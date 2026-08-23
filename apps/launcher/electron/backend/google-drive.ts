import { createHash, randomBytes } from "node:crypto";
import http from "node:http";
import fs from "node:fs/promises";
import path from "node:path";
import { Readable } from "node:stream";
import { safeStorage, shell } from "electron";
import { readJson, removeIfExists, writeJson } from "./fs-utils";
import { googleDriveStatePath } from "./paths";
import type { BackupStore, DownloadOptions, RemoteObject, TransferProgress, UploadOptions, UploadSource } from "./backup-types";

const DRIVE_API = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_API = "https://www.googleapis.com/upload/drive/v3";
const FOLDER_MIME = "application/vnd.google-apps.folder";
const ROOT_FOLDER_NAME = "Industrialis Backups";
const CLIENT_ID_ENV = "INDUSTRIALIS_GOOGLE_DRIVE_CLIENT_ID";
const DRIVE_SCOPE = "https://www.googleapis.com/auth/drive.file";
const CHUNK_SIZE = 8 * 1024 * 1024;

interface GoogleDriveState {
  schema_version: 1;
  client_id?: string;
  refresh_token_encrypted?: string;
  root_folder_id?: string;
}

export interface GoogleDriveStatus {
  configured: boolean;
  connected: boolean;
  client_id?: string;
}

interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
  size?: string;
  modifiedTime?: string;
}

interface DriveListResponse {
  files?: DriveFile[];
  nextPageToken?: string;
}

interface DriveTokenResponse {
  access_token?: string;
  refresh_token?: string;
  expires_in?: number;
  token_type?: string;
}

function clientId(): string {
  return process.env[CLIENT_ID_ENV]?.trim() ?? "";
}

function encodeRefreshToken(token: string): string {
  if (!safeStorage.isEncryptionAvailable()) throw new Error("secure credential storage is unavailable on this system");
  return safeStorage.encryptString(token).toString("base64");
}

function decodeRefreshToken(value: string): string {
  if (!safeStorage.isEncryptionAvailable()) throw new Error("secure credential storage is unavailable on this system");
  return safeStorage.decryptString(Buffer.from(value, "base64"));
}

async function loadState(): Promise<GoogleDriveState | null> {
  const value = await readJson<Partial<GoogleDriveState>>(googleDriveStatePath());
  if (value?.schema_version !== 1 || (typeof value.client_id !== "string" && typeof value.refresh_token_encrypted !== "string")) return null;
  return value as GoogleDriveState;
}

async function saveState(state: GoogleDriveState): Promise<void> {
  await writeJson(googleDriveStatePath(), state);
}

async function responseError(response: Response): Promise<Error> {
  const body = await response.text().catch(() => "");
  let message = body;
  try {
    const parsed = JSON.parse(body) as { error?: { message?: string } | string };
    if (typeof parsed.error === "string") message = parsed.error;
    else if (parsed.error?.message) message = parsed.error.message;
  } catch {
    // Keep the response body when it was not JSON.
  }
  return new Error(`Google Drive request failed: HTTP ${response.status}${message ? ` - ${message}` : ""}`);
}

async function jsonResponse<T>(response: Response): Promise<T> {
  if (!response.ok) throw await responseError(response);
  return (await response.json()) as T;
}

function normalizeKey(key: string): string {
  const normalized = key.replaceAll("\\", "/").replace(/^\/+|\/+$/gu, "");
  if (!normalized || normalized.split("/").some((part) => !part || part === "." || part === "..")) throw new Error("invalid backup key");
  return normalized;
}

function splitKey(key: string): string[] {
  return normalizeKey(key).split("/");
}

async function openGoogleAuthorization(client: string): Promise<{ code: string; redirectUri: string; verifier: string }> {
  const verifier = randomBytes(32).toString("base64url");
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  const state = randomBytes(24).toString("hex");
  const server = http.createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });
  const address = server.address();
  const port = typeof address === "object" && address ? address.port : 0;
  if (!port) throw new Error("could not open a local OAuth callback port");
  const redirectUri = `http://127.0.0.1:${port}/oauth2/callback`;
  const code = new Promise<string>((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Google authorization timed out")), 5 * 60 * 1000);
    server.on("request", (request, response) => {
      const requestUrl = new URL(request.url ?? "/", redirectUri);
      if (requestUrl.pathname !== "/oauth2/callback") {
        response.writeHead(404).end();
        return;
      }
      if (requestUrl.searchParams.get("state") !== state) {
        response.writeHead(400).end("OAuth state mismatch");
        clearTimeout(timeout);
        reject(new Error("Google OAuth state mismatch"));
        return;
      }
      const oauthError = requestUrl.searchParams.get("error");
      if (oauthError) {
        response.writeHead(400).end("Google authorization was cancelled");
        clearTimeout(timeout);
        reject(new Error(`Google authorization failed: ${oauthError}`));
        return;
      }
      const authorizationCode = requestUrl.searchParams.get("code");
      if (!authorizationCode) {
        response.writeHead(400).end("Google authorization did not return a code");
        clearTimeout(timeout);
        reject(new Error("Google authorization did not return a code"));
        return;
      }
      response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" }).end("<p>Google Drive connected. You can close this window.</p>");
      clearTimeout(timeout);
      resolve(authorizationCode);
    });
  });
  const authorizationUrl = new URL("https://accounts.google.com/o/oauth2/v2/auth");
  authorizationUrl.search = new URLSearchParams({
    client_id: client,
    redirect_uri: redirectUri,
    response_type: "code",
    scope: DRIVE_SCOPE,
    access_type: "offline",
    prompt: "consent",
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
  }).toString();
  try {
    await shell.openExternal(authorizationUrl.toString());
    return { code: await code, redirectUri, verifier };
  } finally {
    server.close();
  }
}

async function exchangeAuthorizationCode(client: string, authorization: { code: string; redirectUri: string; verifier: string }): Promise<DriveTokenResponse> {
  const body = new URLSearchParams({
    client_id: client,
    code: authorization.code,
    code_verifier: authorization.verifier,
    grant_type: "authorization_code",
    redirect_uri: authorization.redirectUri,
  });
  const response = await fetch("https://oauth2.googleapis.com/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body,
  });
  return jsonResponse<DriveTokenResponse>(response);
}

export class GoogleDriveAdapter implements BackupStore {
  async getStatus(): Promise<GoogleDriveStatus> {
    const state = await loadState();
    const configuredClientId = clientId() || state?.client_id;
    return {
      configured: Boolean(configuredClientId),
      connected: Boolean(configuredClientId && state?.refresh_token_encrypted),
      ...(configuredClientId ? { client_id: configuredClientId } : {}),
    };
  }

  async configure(rawClientId: string): Promise<GoogleDriveStatus> {
    const configuredClientId = rawClientId.trim();
    if (!configuredClientId || configuredClientId.length < 16) throw new Error("enter a valid Google OAuth desktop client ID");
    await saveState({ schema_version: 1, client_id: configuredClientId });
    return this.getStatus();
  }

  async connect(): Promise<GoogleDriveStatus> {
    const state = await loadState();
    const client = clientId() || state?.client_id || "";
    if (!client) throw new Error(`Google Drive is not configured. Set ${CLIENT_ID_ENV} before starting the launcher.`);
    const authorization = await openGoogleAuthorization(client);
    const token = await exchangeAuthorizationCode(client, authorization);
    if (!token.refresh_token) throw new Error("Google did not return a refresh token; try connecting again and approve access");
    await saveState({
      schema_version: 1,
      client_id: client,
      refresh_token_encrypted: encodeRefreshToken(token.refresh_token),
      root_folder_id: state?.root_folder_id,
    });
    return this.getStatus();
  }

  async disconnect(): Promise<GoogleDriveStatus> {
    const state = await loadState();
    if (state?.client_id) await saveState({ schema_version: 1, client_id: state.client_id });
    else await removeIfExists(googleDriveStatePath());
    return this.getStatus();
  }

  async list(prefix: string): Promise<RemoteObject[]> {
    const segments = splitKey(prefix);
    const parentId = await this.resolveFolder(segments, false);
    if (!parentId) return [];
    const result: RemoteObject[] = [];
    await this.collectObjects(parentId, normalizeKey(prefix), result);
    return result;
  }

  async upload(key: string, source: UploadSource, options: UploadOptions = {}): Promise<RemoteObject> {
    const segments = splitKey(key);
    const name = segments.pop();
    if (!name) throw new Error("backup key has no file name");
    const parentId = await this.resolveFolder(segments, true);
    if (!parentId) throw new Error("Google Drive backup folder could not be created");
    const existing = await this.findChild(parentId, name, false);
    const total = source.kind === "file" ? (await fs.stat(source.path)).size : source.data.byteLength;
    const metadata = {
      name,
      ...(existing ? {} : { parents: [parentId] }),
      mimeType: options.content_type ?? "application/octet-stream",
      appProperties: { industrialis_backup: "true", industrialis_key: normalizeKey(key) },
    };
    const endpoint = existing
      ? `${DRIVE_UPLOAD_API}/files/${existing.id}?uploadType=resumable&fields=id,name,mimeType,size,modifiedTime`
      : `${DRIVE_UPLOAD_API}/files?uploadType=resumable&fields=id,name,mimeType,size,modifiedTime`;
    const sessionResponse = await this.authorizedFetch(endpoint, {
      method: existing ? "PATCH" : "POST",
      headers: { "Content-Type": "application/json; charset=UTF-8", "X-Upload-Content-Type": metadata.mimeType, "X-Upload-Content-Length": String(total) },
      body: JSON.stringify(metadata),
    });
    if (!sessionResponse.ok) throw await responseError(sessionResponse);
    const sessionUrl = sessionResponse.headers.get("location");
    if (!sessionUrl) throw new Error("Google Drive did not return an upload session");
    const uploaded = await this.uploadChunks(sessionUrl, source, total, options.on_progress);
    return {
      key: normalizeKey(key),
      size_bytes: uploaded.size ? Number(uploaded.size) : total,
      modified_at: uploaded.modifiedTime,
      content_type: uploaded.mimeType,
    };
  }

  async download(key: string, destination: string, options: DownloadOptions = {}): Promise<void> {
    const file = await this.findObject(key);
    if (!file) throw new Error("cloud backup object was not found");
    const response = await this.authorizedFetch(`${DRIVE_API}/files/${encodeURIComponent(file.id)}?alt=media`, { method: "GET" });
    if (!response.ok) throw await responseError(response);
    if (!response.body) throw new Error("Google Drive returned an empty download");
    await fs.mkdir(path.dirname(destination), { recursive: true });
    const output = await fs.open(destination, "w");
    const total = file.size ? Number(file.size) : 0;
    let completed = 0;
    try {
      for await (const chunk of Readable.fromWeb(response.body as never)) {
        const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk as Uint8Array);
        await output.write(buffer);
        completed += buffer.length;
        options.on_progress?.({ completed_bytes: completed, total_bytes: total || completed });
      }
    } finally {
      await output.close();
    }
  }

  async delete(key: string): Promise<void> {
    const file = await this.findObject(key);
    if (!file) return;
    const response = await this.authorizedFetch(`${DRIVE_API}/files/${encodeURIComponent(file.id)}`, { method: "DELETE" });
    if (!response.ok) throw await responseError(response);
  }

  private async accessToken(): Promise<string> {
    const state = await loadState();
    const client = clientId() || state?.client_id || "";
    if (!client) throw new Error(`Google Drive is not configured. Set ${CLIENT_ID_ENV} before starting the launcher.`);
    if (!state?.refresh_token_encrypted) throw new Error("Connect Google Drive before using cloud backups");
    const refreshToken = decodeRefreshToken(state.refresh_token_encrypted);
    const response = await fetch("https://oauth2.googleapis.com/token", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ client_id: client, refresh_token: refreshToken, grant_type: "refresh_token" }),
    });
    const token = await jsonResponse<DriveTokenResponse>(response);
    if (!token.access_token) throw new Error("Google did not return an access token");
    return token.access_token;
  }

  private async authorizedFetch(url: string, init: RequestInit, retry = true): Promise<Response> {
    const token = await this.accessToken();
    const headers = new Headers(init.headers);
    headers.set("Authorization", `Bearer ${token}`);
    const response = await fetch(url, { ...init, headers });
    if (response.status === 401 && retry) return this.authorizedFetch(url, init, false);
    return response;
  }

  private async listChildren(parentId: string): Promise<DriveFile[]> {
    const files: DriveFile[] = [];
    let pageToken = "";
    do {
      const query = `'${parentId}' in parents and trashed = false`;
      const url = new URL(`${DRIVE_API}/files`);
      url.searchParams.set("q", query);
      url.searchParams.set("spaces", "drive");
      url.searchParams.set("pageSize", "1000");
      url.searchParams.set("fields", "nextPageToken,files(id,name,mimeType,size,modifiedTime)");
      if (pageToken) url.searchParams.set("pageToken", pageToken);
      const response = await this.authorizedFetch(url.toString(), { method: "GET" });
      const page = await jsonResponse<DriveListResponse>(response);
      files.push(...(page.files ?? []));
      pageToken = page.nextPageToken ?? "";
    } while (pageToken);
    return files;
  }

  private async findChild(parentId: string, name: string, folder: boolean): Promise<DriveFile | null> {
    const children = await this.listChildren(parentId);
    return children.find((entry) => entry.name === name && (folder ? entry.mimeType === FOLDER_MIME : entry.mimeType !== FOLDER_MIME)) ?? null;
  }

  private async createFolder(parentId: string, name: string): Promise<DriveFile> {
    const response = await this.authorizedFetch(`${DRIVE_API}/files?fields=id,name,mimeType`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, mimeType: FOLDER_MIME, parents: [parentId] }),
    });
    return jsonResponse<DriveFile>(response);
  }

  private async rootFolder(create: boolean): Promise<string | null> {
    const state = await loadState();
    if (!state) throw new Error("Connect Google Drive before using cloud backups");
    if (state.root_folder_id) return state.root_folder_id;
    const existing = await this.findChild("root", ROOT_FOLDER_NAME, true);
    const folder = existing ?? (create ? await this.createFolder("root", ROOT_FOLDER_NAME) : null);
    if (!folder) return null;
    await saveState({ ...state, root_folder_id: folder.id });
    return folder.id;
  }

  private async resolveFolder(segments: string[], create: boolean): Promise<string | null> {
    let parentId = await this.rootFolder(create);
    if (!parentId) return null;
    for (const segment of segments) {
      const existing = await this.findChild(parentId, segment, true);
      if (!existing) {
        if (!create) return null;
        parentId = (await this.createFolder(parentId, segment)).id;
      } else {
        parentId = existing.id;
      }
    }
    return parentId;
  }

  private async findObject(key: string): Promise<DriveFile | null> {
    const segments = splitKey(key);
    const name = segments.pop();
    if (!name) return null;
    const parentId = await this.resolveFolder(segments, false);
    return parentId ? this.findChild(parentId, name, false) : null;
  }

  private async collectObjects(parentId: string, prefix: string, result: RemoteObject[]): Promise<void> {
    for (const child of await this.listChildren(parentId)) {
      const key = `${prefix}/${child.name}`;
      if (child.mimeType === FOLDER_MIME) await this.collectObjects(child.id, key, result);
      else result.push({ key, size_bytes: child.size ? Number(child.size) : undefined, modified_at: child.modifiedTime, content_type: child.mimeType });
    }
  }

  private async uploadChunks(sessionUrl: string, source: UploadSource, total: number, onProgress?: (progress: TransferProgress) => void): Promise<DriveFile> {
    const handle = source.kind === "file" ? await fs.open(source.path, "r") : null;
    let offset = 0;
    let finalFile: DriveFile | undefined;
    try {
      while (offset < total || (total === 0 && offset === 0)) {
        const length = total === 0 ? 0 : Math.min(CHUNK_SIZE, total - offset);
        const buffer = Buffer.alloc(length);
        let bytesRead = length;
        if (handle) {
          const read = await handle.read(buffer, 0, length, offset);
          bytesRead = read.bytesRead;
        } else if (source.kind === "bytes") {
          Buffer.from(source.data).copy(buffer, 0, offset, offset + length);
        }
        const end = total === 0 ? -1 : offset + bytesRead - 1;
        const response = await this.putChunk(sessionUrl, buffer.subarray(0, bytesRead), offset, end, total);
        if (response.file) {
          finalFile = response.file;
          offset = total;
        } else {
          offset = response.next_offset;
        }
        onProgress?.({ completed_bytes: offset, total_bytes: total });
        if (total === 0) break;
      }
    } finally {
      await handle?.close();
    }
    if (!finalFile) throw new Error("Google Drive upload did not complete");
    return finalFile;
  }

  private async putChunk(sessionUrl: string, buffer: Buffer, start: number, end: number, total: number): Promise<{ next_offset: number; file?: DriveFile }> {
    for (let attempt = 0; attempt < 4; attempt += 1) {
      const response = await fetch(sessionUrl, {
        method: "PUT",
        headers: {
          "Content-Length": String(buffer.length),
          "Content-Range": total === 0 ? "bytes */0" : `bytes ${start}-${end}/${total}`,
        },
        body: buffer as unknown as BodyInit,
      });
      if (response.status === 308) {
        const range = response.headers.get("range");
        const lastByte = range?.match(/bytes=\d+-(\d+)/u)?.[1];
        return { next_offset: lastByte ? Number(lastByte) + 1 : start + buffer.length };
      }
      if (response.ok) return { next_offset: total, file: (await response.json()) as DriveFile };
      if (response.status >= 500 && attempt < 3) {
        await new Promise((resolve) => setTimeout(resolve, 500 * (attempt + 1)));
        continue;
      }
      throw await responseError(response);
    }
    throw new Error("Google Drive upload failed after retries");
  }
}
