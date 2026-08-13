import crypto from "node:crypto";
import { shell } from "electron";
import { accountsPath } from "./paths";
import { readJson, writeJson } from "./fs-utils";
import type { AccountData, AccountInfo, DeviceCodeInfo, MinecraftEntitlement, MinecraftProfile, MsaToken, StoredToken } from "./types";

const clientId = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";
const scopes = "XboxLive.SignIn XboxLive.offline_access";
const msaTokenUrl = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const msaDeviceCodeUrl = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const refreshWindowSeconds = 12 * 60 * 60;

let pendingOauth: { state: string; resolve: (code: string) => void; reject: (error: Error) => void } | null = null;
let activeMicrosoftLogin: { controller: AbortController } | null = null;

function microsoftLoginCancelledError(): Error {
  const error = new Error("Microsoft sign-in was cancelled");
  error.name = "AbortError";
  return error;
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) throw microsoftLoginCancelledError();
}

async function waitForPoll(milliseconds: number, signal: AbortSignal): Promise<void> {
  throwIfAborted(signal);
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", handleAbort);
      resolve();
    }, milliseconds);
    const handleAbort = () => {
      clearTimeout(timer);
      reject(microsoftLoginCancelledError());
    };
    signal.addEventListener("abort", handleAbort, { once: true });
  });
}

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

async function upsertAccount(account: AccountData, signal?: AbortSignal): Promise<void> {
  throwIfAborted(signal);
  const accounts = await loadAccounts();
  throwIfAborted(signal);
  const profile = profileId(account);
  const next = accounts.filter((entry) => entry.id !== account.id && (!profile || profileId(entry) !== profile));
  next.push(account);
  throwIfAborted(signal);
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

async function requestJson(url: string, init: RequestInit): Promise<{ status: number; body: any }> {
  const response = await fetch(url, init);
  const status = response.status;
  if (!response.ok) {
    const text = await response.text();
    let body: any = {};
    try {
      body = text ? JSON.parse(text) : {};
    } catch {
      body = { raw: text };
    }
    return { status, body };
  }
  const text = await response.text();
  let body: any = {};
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = { raw: text };
  }
  return { status, body };
}

async function requestDeviceCode(signal: AbortSignal): Promise<any> {
  const result = await requestJson(msaDeviceCodeUrl, {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ client_id: clientId, scope: scopes }),
    signal,
  });
  throwIfAborted(signal);
  if (result.body.error) throw new Error(`device code (${result.body.error}): ${result.body.error_description ?? ""}`);
  return result.body;
}

function emitDeviceCode(emit: (event: string, payload: unknown) => void, device: any): void {
  const payload: DeviceCodeInfo = {
    user_code: String(device.user_code),
    verification_uri: String(device.verification_uri ?? device.verification_uri_complete ?? "https://microsoft.com/devicelogin"),
    message: String(device.message ?? "Complete sign-in in your browser."),
  };
  emit("auth-device-code", payload);
}

async function pollDeviceCode(device: any, signal: AbortSignal): Promise<MsaToken> {
  const deadline = Date.now() + Number(device.expires_in ?? 900) * 1000;
  let interval = Math.max(Number(device.interval ?? 5), 5);
  while (Date.now() < deadline) {
    await waitForPoll(interval * 1000, signal);
    const result = await requestJson(msaTokenUrl, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        client_id: clientId,
        grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        device_code: String(device.device_code),
      }),
      signal,
    });
    throwIfAborted(signal);
    if (result.body.access_token) {
      return {
        access_token: result.body.access_token,
        refresh_token: result.body.refresh_token,
        expires_at: expiresIn(Number(result.body.expires_in ?? 3600)),
      };
    }
    switch (result.body.error) {
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
        throw new Error(`device code poll failed (${result.body.error ?? "unknown"}): ${result.body.error_description ?? ""}`);
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

async function xboxUserAuth(msaAccess: string, signal?: AbortSignal): Promise<any> {
  const result = await requestJson("https://user.auth.xboxlive.com/user/authenticate", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({
      Properties: { AuthMethod: "RPS", SiteName: "user.auth.xboxlive.com", RpsTicket: `d=${msaAccess}` },
      RelyingParty: "http://auth.xboxlive.com",
      TokenType: "JWT",
    }),
    signal,
  });
  if (!result.body.Token) throw new Error(`Xbox user auth failed: ${JSON.stringify(result.body)}`);
  return result.body;
}

function xblUhs(body: any): string {
  const uhs = body?.DisplayClaims?.xui?.[0]?.uhs;
  if (typeof uhs !== "string") throw new Error("no uhs in Xbox user token");
  return uhs;
}

async function xstsAuth(userToken: StoredToken, signal?: AbortSignal): Promise<any> {
  const result = await requestJson("https://xsts.auth.xboxlive.com/xsts/authorize", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json", "x-xbl-contract-version": "1" },
    body: JSON.stringify({
      Properties: { SandboxId: "RETAIL", UserTokens: [userToken.token] },
      RelyingParty: "rp://api.minecraftservices.com/",
      TokenType: "JWT",
    }),
    signal,
  });
  if (result.status < 200 || result.status >= 300 || !result.body.Token) {
    const code = Number(result.body.XErr);
    if (code === 2148916233) throw new Error("This Microsoft account has no Xbox profile. Create one at https://www.xbox.com first.");
    if (code === 2148916238) throw new Error("This account is a child account and must be added to a family.");
    if (code === 2148916235) throw new Error("Xbox Live is unavailable in your region.");
    if (code === 2148916236 || code === 2148916237) throw new Error("This account needs adult verification on Xbox Live.");
    throw new Error(`Xbox authorization failed${Number.isFinite(code) ? ` (XErr ${code})` : ""}: ${JSON.stringify(result.body)}`);
  }
  return result.body;
}

async function minecraftLogin(uhs: string, xstsToken: string, signal?: AbortSignal): Promise<any> {
  const identity = `XBL3.0 x=${uhs};${xstsToken}`;
  const primary = await requestJson("https://api.minecraftservices.com/launcher/login", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ xtoken: identity, platform: "PC_LAUNCHER" }),
    signal,
  });
  if (primary.body.access_token) return primary.body;
  if (primary.body.error !== "FORBIDDEN") throw new Error(`Minecraft launcher login failed: ${JSON.stringify(primary.body)}`);
  const fallback = await requestJson("https://api.minecraftservices.com/authentication/login_with_xbox", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ identityToken: identity }),
    signal,
  });
  if (fallback.body.access_token) return fallback.body;
  throw new Error(`Minecraft API rejected this Azure application: ${JSON.stringify(primary.body)}`);
}

async function checkEntitlements(token: string, signal?: AbortSignal): Promise<MinecraftEntitlement> {
  const result = await requestJson("https://api.minecraftservices.com/entitlements/license?requestId=00000000-0000-0000-0000-000000000000", {
    headers: { Authorization: `Bearer ${token}` },
    signal,
  });
  if (result.status < 200 || result.status >= 300) throw new Error(`entitlements check failed: ${JSON.stringify(result.body)}`);
  let owns = false;
  let canPlay = false;
  for (const item of result.body.items ?? []) {
    if (["product_minecraft", "game_minecraft"].includes(item.name)) owns = true;
    if (["product_minecraft", "game_minecraft", "product_game_pass_pc"].includes(item.name)) canPlay = true;
  }
  return { owns_minecraft: owns, can_play_minecraft: canPlay };
}

async function fetchProfile(token: string, signal?: AbortSignal): Promise<MinecraftProfile | undefined> {
  const result = await requestJson("https://api.minecraftservices.com/minecraft/profile", { headers: { Authorization: `Bearer ${token}` }, signal });
  if (result.status === 404) return undefined;
  if (result.status < 200 || result.status >= 300) throw new Error(`profile fetch failed: ${JSON.stringify(result.body)}`);
  return result.body as MinecraftProfile;
}

async function downloadSkin(profile?: MinecraftProfile, signal?: AbortSignal): Promise<string | undefined> {
  const skin = profile?.skins?.find((entry) => entry.state === "ACTIVE") ?? profile?.skins?.[0];
  if (!skin?.url) return undefined;
  const response = await fetch(skin.url, { signal });
  if (!response.ok) return undefined;
  return Buffer.from(await response.arrayBuffer()).toString("base64");
}

async function runPipeline(msa: MsaToken, signal?: AbortSignal): Promise<AccountData> {
  throwIfAborted(signal);
  const account: AccountData = { format_version: 3, account_type: "msa", id: crypto.randomUUID(), msa_token: msa };
  const xbl = await xboxUserAuth(msa.access_token, signal);
  throwIfAborted(signal);
  const userToken: StoredToken = { token: xbl.Token, expires_at: parseXboxExpiry(xbl.NotAfter), extra: { uhs: xblUhs(xbl) } };
  account.user_token = userToken;
  const xsts = await xstsAuth(userToken, signal);
  throwIfAborted(signal);
  const mojangToken: StoredToken = { token: xsts.Token, expires_at: parseXboxExpiry(xsts.NotAfter), extra: { uhs: xblUhs(xsts) } };
  account.mojangservices_token = mojangToken;
  const minecraft = await minecraftLogin(tokenUhs(mojangToken) ?? "", mojangToken.token, signal);
  throwIfAborted(signal);
  account.yggdrasil_token = { token: minecraft.access_token, expires_at: expiresIn(Number(minecraft.expires_in ?? 86400)) };
  account.minecraft_entitlement = await checkEntitlements(accessToken(account), signal);
  throwIfAborted(signal);
  account.minecraft_profile = await fetchProfile(accessToken(account), signal);
  throwIfAborted(signal);
  account.skin_png_base64 = await downloadSkin(account.minecraft_profile, signal);
  throwIfAborted(signal);
  return account;
}

export async function startMicrosoftLogin(emit: (event: string, payload: unknown) => void): Promise<AccountInfo> {
  if (activeMicrosoftLogin) throw new Error("Microsoft sign-in is already in progress");

  const attempt = { controller: new AbortController() };
  activeMicrosoftLogin = attempt;
  const { signal } = attempt.controller;

  try {
    const device = await requestDeviceCode(signal);
    throwIfAborted(signal);
    emitDeviceCode(emit, device);
    void shell.openExternal(String(device.verification_uri ?? "https://microsoft.com/devicelogin"));
    const account = await runPipeline(await pollDeviceCode(device, signal), signal);
    await upsertAccount(account, signal);
    throwIfAborted(signal);
    return accountInfo(account);
  } finally {
    if (activeMicrosoftLogin === attempt) activeMicrosoftLogin = null;
  }
}

export function cancelMicrosoftLogin(): void {
  const attempt = activeMicrosoftLogin;
  if (!attempt) return;
  activeMicrosoftLogin = null;
  attempt.controller.abort();
}

export async function ensureFreshToken(account: AccountData): Promise<string> {
  const msa = account.msa_token;
  if (!msa || msa.expires_at - now() > refreshWindowSeconds) return accessToken(account);
  const result = await requestJson(msaTokenUrl, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ client_id: clientId, refresh_token: msa.refresh_token, grant_type: "refresh_token", scope: scopes }),
  });
  if (!result.body.access_token) throw new Error(`MSA refresh failed: ${JSON.stringify(result.body)}`);
  const refreshed = await runPipeline({
    access_token: result.body.access_token,
    refresh_token: result.body.refresh_token ?? msa.refresh_token,
    expires_at: expiresIn(Number(result.body.expires_in ?? 3600)),
  });
  refreshed.id = account.id;
  await upsertAccount(refreshed);
  return accessToken(refreshed);
}

export function handleOauthCallback(url: string): void {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return;
  }
  if (parsed.protocol !== "industrialislauncher:" || parsed.hostname !== "oauth" || !/^\/microsoft\/?$/u.test(parsed.pathname)) return;
  if (!pendingOauth) return;
  const current = pendingOauth;
  pendingOauth = null;
  const error = parsed.searchParams.get("error");
  if (error) current.reject(new Error(`Microsoft login error (${error}): ${parsed.searchParams.get("error_description") ?? ""}`));
  else if (parsed.searchParams.get("state") !== current.state) current.reject(new Error("OAuth state mismatch"));
  else current.resolve(parsed.searchParams.get("code") ?? "");
}

export function accountToInfo(account: AccountData): AccountInfo {
  return accountInfo(account);
}
