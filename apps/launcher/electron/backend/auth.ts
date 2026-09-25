import crypto from "node:crypto";
import { shell } from "electron";
import { formatHttpResult, isTransientHttpStatus, requestJson, retryWithBackoff, sleep, DEFAULT_REQUEST_TIMEOUT_MS } from "./http";
import type { JsonResult } from "./http";
import { accountsPath } from "./paths";
import { readJson, writeJson } from "./fs-utils";
import type { AccountData, AccountInfo, DeviceCodeInfo, MinecraftEntitlement, MinecraftProfile, MsaToken, StoredToken } from "./types";

interface DeviceCodeResponse {
  device_code: string;
  user_code: string;
  verification_uri?: string;
  verification_uri_complete?: string;
  message?: string;
  expires_in?: number | string;
  interval?: number | string;
  error?: string;
  error_description?: string;
}

interface TokenPollBody {
  access_token?: string;
  refresh_token?: string;
  expires_in?: number | string;
  error?: string;
  error_description?: string;
}

interface XboxAuthBody {
  Token?: string;
  NotAfter?: string;
  DisplayClaims?: { xui?: Array<{ uhs?: string }> };
  XErr?: number | string;
}

interface MinecraftLoginBody {
  access_token?: string;
  expires_in?: number | string;
  error?: string;
}

interface EntitlementsBody {
  items?: Array<{ name?: string }>;
}

/**
 * Retry transport-level failures (thrown by requestJson on timeouts/network
 * errors). HTTP error statuses are returned, not thrown, so they never
 * trigger a retry here — callers handle statuses explicitly.
 */
async function resilientRequest(url: string, init: RequestInit, options: { timeoutMs?: number } = {}): Promise<JsonResult> {
  return retryWithBackoff(() => requestJson(url, init, options), { maxAttempts: 3, baseDelayMs: 500 });
}

const clientId = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";
const scopes = "XboxLive.SignIn XboxLive.offline_access";
const msaTokenUrl = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const msaDeviceCodeUrl = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const refreshWindowSeconds = 12 * 60 * 60;

function now(): number {
  return Math.floor(Date.now() / 1000);
}
function expiresIn(seconds: number): number {
  return now() + seconds;
}

export async function loadAccounts(): Promise<AccountData[]> {
  const raw = await readJson<unknown>(accountsPath());
  if (Array.isArray(raw)) {
    if (raw.every((account) => typeof account === "object" && account !== null && "format_version" in account)) return raw as AccountData[];
    if (raw.every((account) => typeof account === "object" && account !== null && "access_token" in account)) {
      return (raw as Array<{ id: string; username: string; uuid: string; access_token: string; refresh_token: string; expires_at: number }>).map((legacy) => ({
        format_version: 3,
        account_type: "msa",
        id: legacy.id,
        msa_token: { access_token: "", refresh_token: legacy.refresh_token, expires_at: legacy.expires_at },
        yggdrasil_token: { token: legacy.access_token, expires_at: 0 },
        minecraft_profile: { id: legacy.uuid, name: legacy.username },
      }));
    }
  }
  return [];
}

export async function saveAccounts(accounts: AccountData[]): Promise<void> {
  await writeJson(accountsPath(), accounts);
}

function accountInfo(account: AccountData): AccountInfo {
  return {
    id: account.id,
    username: account.minecraft_profile?.name ?? "",
    uuid: account.minecraft_profile?.id ?? "",
    account_type: account.account_type,
    ...(account.skin_png_base64 ? { skin_png_base64: account.skin_png_base64 } : {}),
    ...(account.minecraft_entitlement
      ? {
          owns_minecraft: account.minecraft_entitlement.owns_minecraft,
          can_play_minecraft: account.minecraft_entitlement.can_play_minecraft,
        }
      : {}),
  };
}

function isOffline(account: AccountData): boolean {
  return account.account_type === "offline";
}
function accessToken(account: AccountData): string {
  return account.yggdrasil_token?.token ?? "";
}
function profileName(account: AccountData): string {
  return account.minecraft_profile?.name ?? "";
}
function profileId(account: AccountData): string {
  return account.minecraft_profile?.id ?? "";
}

async function upsertAccount(account: AccountData): Promise<void> {
  const accounts = await loadAccounts();
  const profile = profileId(account);
  const next = accounts.filter((entry) => entry.id !== account.id && (!profile || profileId(entry) !== profile));
  next.push(account);
  await saveAccounts(next);
}

export function validateOfflineUsername(value: string): string {
  const username = value.trim();
  if (!username) throw new Error("username cannot be empty");
  if (username.length > 16) throw new Error("username must be 16 characters or fewer");
  if (!/^[A-Za-z0-9_]+$/u.test(username)) throw new Error("username may only contain letters, numbers, and underscores");
  return username;
}

export function offlinePlayerUuid(username: string): string {
  const digest = crypto.createHash("md5").update(`OfflinePlayer:${username}`).digest();
  digest[6] = (digest[6] & 0x0f) | 0x30;
  digest[8] = (digest[8] & 0x3f) | 0x80;
  const hex = digest.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export async function createOfflineAccount(usernameRaw: string): Promise<AccountInfo> {
  const username = validateOfflineUsername(usernameRaw);
  const accounts = await loadAccounts();
  if (accounts.some((account) => isOffline(account) && profileName(account).toLowerCase() === username.toLowerCase())) {
    throw new Error("an offline account with this username already exists");
  }
  const account: AccountData = {
    format_version: 3,
    account_type: "offline",
    id: crypto.randomUUID(),
    minecraft_profile: { id: offlinePlayerUuid(username), name: username },
  };
  await upsertAccount(account);
  return accountInfo(account);
}

async function requestDeviceCode(): Promise<DeviceCodeResponse> {
  const result = await resilientRequest(msaDeviceCodeUrl, {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ client_id: clientId, scope: scopes }),
  });
  const body = result.body as DeviceCodeResponse;
  if (body.error) throw new Error(`device code (${body.error}): ${body.error_description ?? ""}`);
  if (result.status < 200 || result.status >= 300 || !body.device_code) {
    throw new Error(`device code request failed: ${formatHttpResult(result)}`);
  }
  return body;
}

function emitDeviceCode(emit: (event: string, payload: unknown) => void, device: DeviceCodeResponse): void {
  const payload: DeviceCodeInfo = {
    user_code: String(device.user_code),
    verification_uri: String(device.verification_uri ?? device.verification_uri_complete ?? "https://microsoft.com/devicelogin"),
    message: String(device.message ?? "Complete sign-in in your browser."),
  };
  emit("auth-device-code", payload);
}

async function pollDeviceCode(device: DeviceCodeResponse): Promise<MsaToken> {
  // Bespoke long-poll on purpose: the server directs pacing via `interval` /
  // `slow_down`, the deadline comes from `expires_in`, and most responses are
  // non-terminal states (authorization_pending) rather than failures — none
  // of which fits the generic retryWithBackoff (fixed backoff, throw-to-retry)
  // helper shared with Drive chunk uploads.
  const deadline = Date.now() + Number(device.expires_in ?? 900) * 1000;
  let interval = Math.max(Number(device.interval ?? 5), 5);
  let networkFailures = 0;
  while (Date.now() < deadline) {
    await sleep(interval * 1000);
    let result: JsonResult;
    try {
      result = await requestJson(msaTokenUrl, {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({
          client_id: clientId,
          grant_type: "urn:ietf:params:oauth:grant-type:device_code",
          device_code: String(device.device_code),
        }),
      });
    } catch (error) {
      // A dropped poll request shouldn't abort the whole login — the user may
      // still be completing sign-in in their browser. Keep polling, but give
      // up if the network stays down.
      networkFailures += 1;
      if (networkFailures >= 3) throw error;
      continue;
    }
    networkFailures = 0;
    const poll = result.body as TokenPollBody;
    if (poll.access_token) {
      return {
        access_token: poll.access_token,
        refresh_token: poll.refresh_token ?? "",
        expires_at: expiresIn(Number(poll.expires_in ?? 3600)),
      };
    }
    switch (poll.error) {
      case "authorization_pending":
        continue;
      case "slow_down":
        interval += 5;
        continue;
      case "expired_token":
        throw new Error("device code expired — try again");
      case "access_denied":
        throw new Error("login denied");
      default:
        throw new Error(`device code poll failed (HTTP ${result.status}, ${poll.error ?? "unknown"}): ${poll.error_description ?? ""}`);
    }
  }
  throw new Error("device code login timed out");
}

function tokenUhs(token?: StoredToken): string | undefined {
  const extra = token?.extra;
  return typeof extra === "object" && extra !== null && "uhs" in extra && typeof extra.uhs === "string" ? extra.uhs : undefined;
}

function parseXboxExpiry(value: unknown): number {
  if (typeof value !== "string") return 0;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? Math.floor(timestamp / 1000) : 0;
}

async function xboxUserAuth(msaAccess: string): Promise<XboxAuthBody> {
  const result = await resilientRequest("https://user.auth.xboxlive.com/user/authenticate", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({
      Properties: { AuthMethod: "RPS", SiteName: "user.auth.xboxlive.com", RpsTicket: `d=${msaAccess}` },
      RelyingParty: "http://auth.xboxlive.com",
      TokenType: "JWT",
    }),
  });
  const body = result.body as XboxAuthBody;
  if (!body.Token) throw new Error(`Xbox user auth failed: ${formatHttpResult(result)}`);
  return body;
}

function xblUhs(body: XboxAuthBody): string {
  const uhs = body?.DisplayClaims?.xui?.[0]?.uhs;
  if (typeof uhs !== "string") throw new Error("no uhs in Xbox user token");
  return uhs;
}

async function xstsAuth(userToken: StoredToken): Promise<XboxAuthBody> {
  const result = await resilientRequest("https://xsts.auth.xboxlive.com/xsts/authorize", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json", "x-xbl-contract-version": "1" },
    body: JSON.stringify({
      Properties: { SandboxId: "RETAIL", UserTokens: [userToken.token] },
      RelyingParty: "rp://api.minecraftservices.com/",
      TokenType: "JWT",
    }),
  });
  const body = result.body as XboxAuthBody;
  if (result.status < 200 || result.status >= 300 || !body.Token) {
    const code = Number(body.XErr);
    if (code === 2148916233) throw new Error("This Microsoft account has no Xbox profile. Create one at https://www.xbox.com first.");
    if (code === 2148916238) throw new Error("This account is a child account and must be added to a family.");
    if (code === 2148916235) throw new Error("Xbox Live is unavailable in your region.");
    if (code === 2148916236 || code === 2148916237) throw new Error("This account needs adult verification on Xbox Live.");
    throw new Error(`Xbox authorization failed${Number.isFinite(code) ? ` (XErr ${code})` : ""}: ${formatHttpResult(result)}`);
  }
  return body;
}

async function minecraftLogin(uhs: string, xstsToken: string): Promise<MinecraftLoginBody> {
  const identity = `XBL3.0 x=${uhs};${xstsToken}`;
  const primary = await resilientRequest("https://api.minecraftservices.com/launcher/login", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ xtoken: identity, platform: "PC_LAUNCHER" }),
  });
  const primaryBody = primary.body as MinecraftLoginBody;
  if (primaryBody.access_token) return primaryBody;
  // The primary endpoint can fail transiently (notably HTTP 429, whose body
  // is just {"path":"/launcher/login"}) while the legacy endpoint — on a
  // separate rate-limit bucket — still succeeds. Try the fallback only for
  // transient/rate-limit failures and explicit FORBIDDEN rejections; permanent
  // client errors fail fast instead of doubling login calls.
  const fallbackWarranted = isTransientHttpStatus(primary.status) || primaryBody.error === "FORBIDDEN";
  if (!fallbackWarranted) {
    throw new Error(`Minecraft launcher login failed: ${formatHttpResult(primary)}`);
  }
  const fallback = await resilientRequest("https://api.minecraftservices.com/authentication/login_with_xbox", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ identityToken: identity }),
  });
  const fallbackBody = fallback.body as MinecraftLoginBody;
  if (fallbackBody.access_token) return fallbackBody;
  if (primary.status === 429 || fallback.status === 429) {
    throw new Error(
      `Minecraft services are rate-limiting logins (HTTP 429). Wait a minute and try again. [launcher/login ${formatHttpResult(primary)}; login_with_xbox ${formatHttpResult(fallback)}]`,
    );
  }
  if (primaryBody.error === "FORBIDDEN") throw new Error(`Minecraft API rejected this Azure application: ${formatHttpResult(primary)}`);
  throw new Error(`Minecraft launcher login failed: launcher/login ${formatHttpResult(primary)}; login_with_xbox ${formatHttpResult(fallback)}`);
}

async function checkEntitlements(token: string): Promise<MinecraftEntitlement> {
  const result = await resilientRequest("https://api.minecraftservices.com/entitlements/license?requestId=00000000-0000-0000-0000-000000000000", {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`entitlements check failed: ${formatHttpResult(result)}`);
  }
  const body = result.body as EntitlementsBody;
  let owns = false;
  let canPlay = false;
  for (const item of body.items ?? []) {
    if (["product_minecraft", "game_minecraft"].includes(item.name ?? "")) owns = true;
    if (["product_minecraft", "game_minecraft", "product_game_pass_pc"].includes(item.name ?? "")) canPlay = true;
  }
  return { owns_minecraft: owns, can_play_minecraft: canPlay };
}

async function fetchProfile(token: string): Promise<MinecraftProfile | undefined> {
  const result = await resilientRequest("https://api.minecraftservices.com/minecraft/profile", { headers: { Authorization: `Bearer ${token}` } });
  if (result.status === 404) return undefined;
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`profile fetch failed: ${formatHttpResult(result)}`);
  }
  return result.body as MinecraftProfile;
}

async function downloadSkin(profile?: MinecraftProfile): Promise<string | undefined> {
  const skin = profile?.skins?.find((entry) => entry.state === "ACTIVE") ?? profile?.skins?.[0];
  if (!skin?.url) return undefined;
  // Best effort: a skin download failure must never fail the login itself.
  try {
    const response = await fetch(skin.url, { signal: AbortSignal.timeout(DEFAULT_REQUEST_TIMEOUT_MS) });
    if (!response.ok) return undefined;
    return Buffer.from(await response.arrayBuffer()).toString("base64");
  } catch {
    return undefined;
  }
}

async function runPipeline(msa: MsaToken): Promise<AccountData> {
  const account: AccountData = { format_version: 3, account_type: "msa", id: crypto.randomUUID(), msa_token: msa };
  const xbl = await xboxUserAuth(msa.access_token);
  if (!xbl.Token) throw new Error("Xbox user auth returned no token");
  const userToken: StoredToken = { token: xbl.Token, expires_at: parseXboxExpiry(xbl.NotAfter), extra: { uhs: xblUhs(xbl) } };
  account.user_token = userToken;
  const xsts = await xstsAuth(userToken);
  if (!xsts.Token) throw new Error("Xbox authorization returned no token");
  const mojangToken: StoredToken = { token: xsts.Token, expires_at: parseXboxExpiry(xsts.NotAfter), extra: { uhs: xblUhs(xsts) } };
  account.mojangservices_token = mojangToken;
  const minecraft = await minecraftLogin(tokenUhs(mojangToken) ?? "", mojangToken.token);
  if (!minecraft.access_token) throw new Error("Minecraft login returned no access token");
  account.yggdrasil_token = { token: minecraft.access_token, expires_at: expiresIn(Number(minecraft.expires_in ?? 86400)) };
  account.minecraft_entitlement = await checkEntitlements(accessToken(account));
  account.minecraft_profile = await fetchProfile(accessToken(account));
  account.skin_png_base64 = await downloadSkin(account.minecraft_profile);
  return account;
}

export async function startMicrosoftLogin(emit: (event: string, payload: unknown) => void): Promise<AccountInfo> {
  const device = await requestDeviceCode();
  emitDeviceCode(emit, device);
  void shell.openExternal(String(device.verification_uri ?? "https://microsoft.com/devicelogin"));
  const account = await runPipeline(await pollDeviceCode(device));
  await upsertAccount(account);
  return accountInfo(account);
}

export async function ensureFreshToken(account: AccountData): Promise<string> {
  const msa = account.msa_token;
  if (!msa || msa.expires_at - now() > refreshWindowSeconds) return accessToken(account);
  const result = await resilientRequest(msaTokenUrl, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ client_id: clientId, refresh_token: msa.refresh_token, grant_type: "refresh_token", scope: scopes }),
  });
  const refreshedBody = result.body as TokenPollBody;
  if (!refreshedBody.access_token) throw new Error(`MSA refresh failed: ${formatHttpResult(result)}`);
  const refreshed = await runPipeline({
    access_token: refreshedBody.access_token,
    refresh_token: refreshedBody.refresh_token ?? msa.refresh_token,
    expires_at: expiresIn(Number(refreshedBody.expires_in ?? 3600)),
  });
  refreshed.id = account.id;
  await upsertAccount(refreshed);
  return accessToken(refreshed);
}

export function accountToInfo(account: AccountData): AccountInfo {
  return accountInfo(account);
}
