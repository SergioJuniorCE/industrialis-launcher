//! The existing Google Drive provider, folder layout and OAuth state file.
use crate::{
    backups::{BackupStore, Progress, RemoteObject, UploadSource},
    paths::LauncherPaths,
};
use anyhow::{ensure, Context};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
const FOLDER: &str = "application/vnd.google-apps.folder";
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
fn trusted_upload_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "googleapis.com" || host.ends_with(".googleapis.com"))
}
fn resume_offset(headers: &reqwest::header::HeaderMap) -> anyhow::Result<u64> {
    let Some(range) = headers.get("range") else {
        return Ok(0);
    };
    let last = range
        .to_str()?
        .strip_prefix("bytes=0-")
        .and_then(|v| v.parse::<u64>().ok())
        .and_then(|last| last.checked_add(1))
        .context("Google Drive upload offset is invalid")?;
    Ok(last)
}
#[derive(Clone, Debug, Serialize)]
pub struct DriveStatus {
    pub configured: bool,
    pub connected: bool,
    pub client_id: Option<String>,
    pub error: Option<String>,
}
#[derive(Clone, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    size: Option<String>,
}
#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(rename = "nextPageToken")]
    next: Option<String>,
}
pub struct GoogleDrive {
    paths: LauncherPaths,
    client: reqwest::Client,
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
    folders: tokio::sync::Mutex<()>,
    credential_status: tokio::sync::Mutex<Option<(String, Option<String>)>>,
}
impl GoogleDrive {
    pub fn new(paths: LauncherPaths) -> Self {
        Self {
            paths,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() < 5 && trusted_upload_url(attempt.url()) {
                        attempt.follow()
                    } else {
                        attempt.stop()
                    }
                }))
                .build()
                .unwrap(),
            token: Default::default(),
            folders: Default::default(),
            credential_status: Default::default(),
        }
    }
    async fn upload_chunks(
        &self,
        location: reqwest::Url,
        mut file: Option<tokio::fs::File>,
        bytes: Vec<u8>,
        total: u64,
        progress: Progress,
    ) -> anyhow::Result<()> {
        let mut offset = 0;
        let mut retries = 0;
        let mut completed = false;
        while offset < total || (total == 0 && !completed) {
            let length = (total - offset).min(8 * 1024 * 1024) as usize;
            let mut chunk = vec![0; length];
            if let Some(file) = &mut file {
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                file.read_exact(&mut chunk).await?;
            } else {
                chunk.copy_from_slice(&bytes[offset as usize..offset as usize + length]);
            }
            let range = if total == 0 {
                "bytes */0".into()
            } else {
                format!("bytes {}-{}/{total}", offset, offset + length as u64 - 1)
            };
            let response = self
                .client
                .put(location.clone())
                .header("Content-Length", length)
                .header("Content-Range", range)
                .body(chunk)
                .send()
                .await;
            let response = match response {
                Ok(response)
                    if response.status().is_success() || response.status().as_u16() == 308 =>
                {
                    response
                }
                result => {
                    retries += 1;
                    ensure!(
                        retries < 4,
                        "Google Drive upload was interrupted; retry the queued snapshot"
                    );
                    if let Ok(response) = result {
                        ensure!(
                            response.status().is_server_error()
                                || response.status().as_u16() == 429,
                            "Google Drive chunk upload failed (HTTP {})",
                            response.status().as_u16()
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(500 * retries)).await;
                    self.client
                        .put(location.clone())
                        .header("Content-Length", 0)
                        .header("Content-Range", format!("bytes */{total}"))
                        .body(Vec::new())
                        .send()
                        .await?
                }
            };
            if response.status().is_success() {
                completed = true;
                offset = total;
            } else if response.status().as_u16() == 308 {
                let next = resume_offset(response.headers())?;
                ensure!(
                    next >= offset && next <= offset + length as u64,
                    "Google Drive upload offset is invalid"
                );
                if next == offset {
                    retries += 1;
                    ensure!(retries < 4, "Google Drive upload made no progress");
                } else {
                    retries = 0;
                }
                offset = next;
            } else {
                anyhow::bail!(
                    "Google Drive upload recovery failed (HTTP {})",
                    response.status().as_u16()
                );
            }
            progress(offset, total);
            if completed {
                break;
            }
        }
        ensure!(completed, "Google Drive upload did not complete");
        Ok(())
    }
    async fn state(&self) -> anyhow::Result<Value> {
        let value = crate::profile_store::read_object(&self.paths.google_drive_state_path())
            .await?
            .unwrap_or_else(|| json!({"schema_version":1}));
        ensure!(
            value["schema_version"] == 1,
            "Google Drive state uses an unsupported schema; the original has been kept"
        );
        Ok(value)
    }
    fn client_id(state: &Value) -> Option<String> {
        std::env::var("INDUSTRIALIS_GOOGLE_DRIVE_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                state["client_id"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
            })
    }
    pub async fn status(&self) -> anyhow::Result<DriveStatus> {
        let state = self.state().await?;
        let client_id = Self::client_id(&state);
        let mut status = DriveStatus {
            configured: client_id.is_some(),
            connected: false,
            client_id,
            error: None,
        };
        if let Some(encrypted) = state["refresh_token_encrypted"].as_str() {
            let mut cache = self.credential_status.lock().await;
            if !cache.as_ref().is_some_and(|(value, _)| value == encrypted) {
                let paths = self.paths.clone();
                let value = encrypted.to_string();
                let error = tokio::task::spawn_blocking(move || {
                    crate::credentials::decrypt(&paths, &value)
                })
                .await?
                .err()
                .map(|e| format!("{e:#}"));
                *cache = Some((encrypted.to_string(), error));
            }
            status.error = cache.as_ref().and_then(|(_, error)| error.clone());
            status.connected = status.configured && status.error.is_none();
        }
        Ok(status)
    }
    pub async fn refresh_status(&self) -> anyhow::Result<DriveStatus> {
        *self.credential_status.lock().await = None;
        self.status().await
    }
    pub async fn configure(&self, id: String) -> anyhow::Result<DriveStatus> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let id = id.trim().to_string();
        ensure!(
            id.len() >= 16 && id.ends_with(".apps.googleusercontent.com"),
            "Enter a Google OAuth desktop client ID"
        );
        let path = self.paths.google_drive_state_path();
        crate::profile_store::mutate(async move {
            let mut state = crate::profile_store::read_object(&path)
                .await?
                .unwrap_or_else(|| json!({"schema_version":1}));
            ensure!(
                state["schema_version"] == 1,
                "Google Drive state uses an unsupported schema"
            );
            state["client_id"] = id.into();
            state
                .as_object_mut()
                .unwrap()
                .remove("refresh_token_encrypted");
            state.as_object_mut().unwrap().remove("root_folder_id");
            crate::fs_utils::write_json(&path, &state).await
        })
        .await?;
        *self.token.lock().await = None;
        self.status().await
    }
    pub async fn disconnect(&self) -> anyhow::Result<DriveStatus> {
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let path = self.paths.google_drive_state_path();
        crate::profile_store::mutate(async move {
            let Some(mut state) = crate::profile_store::read_object(&path).await? else {
                return Ok(());
            };
            ensure!(
                state["schema_version"] == 1,
                "Google Drive state uses an unsupported schema"
            );
            state
                .as_object_mut()
                .unwrap()
                .remove("refresh_token_encrypted");
            state.as_object_mut().unwrap().remove("root_folder_id");
            crate::fs_utils::write_json(&path, &state).await
        })
        .await?;
        *self.token.lock().await = None;
        self.status().await
    }
    pub async fn connect(&self) -> anyhow::Result<DriveStatus> {
        use ring::rand::SecureRandom;
        let _lease = crate::lifecycle::admit(&self.paths)?;
        let original = self.state().await?;
        let client = Self::client_id(&original)
            .ok_or_else(|| anyhow::anyhow!("Configure Google Drive first"))?;
        let mut random = [0; 32];
        ring::rand::SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| anyhow::anyhow!("Secure random generation failed"))?;
        let verifier = URL_SAFE_NO_PAD.encode(random);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        ring::rand::SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| anyhow::anyhow!("Secure random generation failed"))?;
        let state = URL_SAFE_NO_PAD.encode(random);
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let redirect = format!(
            "http://127.0.0.1:{}/oauth2/callback",
            listener.local_addr()?.port()
        );
        let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", client.as_str()),
            ("redirect_uri", &redirect),
            ("response_type", "code"),
            ("scope", "https://www.googleapis.com/auth/drive.file"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]);
        tokio::task::spawn_blocking(move || open::that(url.as_str())).await??;
        let lifecycle = crate::lifecycle::for_profile(&self.paths);
        let code = tokio::select! {_=lifecycle.closing()=>anyhow::bail!("Google authorization was cancelled while the launcher closed"),result=tokio::time::timeout(Duration::from_secs(300),oauth_callback(listener,&redirect,&state))=>result.context("Google authorization timed out")??};
        let response = self
            .client
            .post(TOKEN)
            .form(&[
                ("client_id", client.as_str()),
                ("code", &code),
                ("code_verifier", &verifier),
                ("grant_type", "authorization_code"),
                ("redirect_uri", &redirect),
            ])
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "Google authorization was denied or expired (HTTP {})",
            response.status().as_u16()
        );
        let token: Value = response.json().await?;
        let refresh = token["refresh_token"]
            .as_str()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Google did not return a refresh token; approve offline access and try again"
                )
            })?
            .to_string();
        let encrypted = crate::credentials::encrypt(&self.paths, refresh).await?;
        let path = self.paths.google_drive_state_path();
        let expected = original.clone();
        let saved_client = client.clone();
        crate::profile_store::mutate(async move {let mut latest=crate::profile_store::read_object(&path).await?.unwrap_or_else(||json!({"schema_version":1}));ensure!(latest==expected,"Google Drive settings changed during authorization; the newer settings have been kept");latest["client_id"]=saved_client.into();latest["refresh_token_encrypted"]=encrypted.into();crate::fs_utils::write_json(&path,&latest).await}).await?;
        *self.token.lock().await = None;
        self.status().await
    }
    async fn access_token(&self) -> anyhow::Result<String> {
        let mut cache = self.token.lock().await;
        if let Some((token, expires)) = &*cache {
            if *expires > Instant::now() {
                return Ok(token.clone());
            }
        }
        let state = self.state().await?;
        let client = Self::client_id(&state)
            .ok_or_else(|| anyhow::anyhow!("Configure Google Drive first"))?;
        let encrypted = state["refresh_token_encrypted"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Connect Google Drive before using cloud backups"))?
            .to_string();
        let paths = self.paths.clone();
        let refresh =
            tokio::task::spawn_blocking(move || crate::credentials::decrypt(&paths, &encrypted))
                .await??;
        let response = self
            .client
            .post(TOKEN)
            .form(&[
                ("client_id", client.as_str()),
                ("refresh_token", &refresh),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "Google Drive authorization expired or was denied (HTTP {})",
            response.status().as_u16()
        );
        let token: Value = response.json().await?;
        let value = token["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Google did not return an access token"))?
            .to_string();
        let seconds = token["expires_in"]
            .as_u64()
            .unwrap_or(3600)
            .saturating_sub(60)
            .max(1);
        *cache = Some((value.clone(), Instant::now() + Duration::from_secs(seconds)));
        Ok(value)
    }
    async fn authorized(
        &self,
        request: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::Response> {
        let retry = request.try_clone();
        let response = request
            .bearer_auth(self.access_token().await?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(retry) = retry {
                *self.token.lock().await = None;
                return Ok(retry.bearer_auth(self.access_token().await?).send().await?);
            }
        }
        Ok(response)
    }
    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> anyhow::Result<T> {
        ensure!(
            response.status().is_success(),
            "Google Drive request failed (HTTP {})",
            response.status().as_u16()
        );
        Ok(response.json().await?)
    }
    async fn children(&self, parent: &str) -> anyhow::Result<Vec<DriveFile>> {
        ensure!(
            !parent.contains(['\'', '\\']),
            "Google Drive folder ID is invalid"
        );
        let mut result = Vec::new();
        let mut page = String::new();
        loop {
            let mut query = vec![
                ("q", format!("'{parent}' in parents and trashed = false")),
                ("spaces", "drive".into()),
                ("pageSize", "1000".into()),
                (
                    "fields",
                    "nextPageToken,files(id,name,mimeType,size)".into(),
                ),
            ];
            if !page.is_empty() {
                query.push(("pageToken", page));
            }
            let response = self
                .authorized(self.client.get(format!("{API}/files")).query(&query))
                .await?;
            let value: Page = self.json(response).await?;
            result.extend(value.files);
            if let Some(next) = value.next {
                page = next;
            } else {
                break;
            }
        }
        Ok(result)
    }
    async fn find_child(
        &self,
        parent: &str,
        name: &str,
        folder: bool,
    ) -> anyhow::Result<Option<DriveFile>> {
        Ok(self
            .children(parent)
            .await?
            .into_iter()
            .find(|file| file.name == name && (file.mime_type == FOLDER) == folder))
    }
    async fn create_folder(&self, parent: &str, name: &str) -> anyhow::Result<DriveFile> {
        let response = self
            .authorized(
                self.client
                    .post(format!("{API}/files"))
                    .query(&[("fields", "id,name,mimeType")])
                    .json(&json!({"name":name,"mimeType":FOLDER,"parents":[parent]})),
            )
            .await?;
        self.json(response).await
    }
    async fn root(&self, create: bool) -> anyhow::Result<Option<String>> {
        let state = self.state().await?;
        if let Some(id) = state["root_folder_id"].as_str() {
            return Ok(Some(id.into()));
        }
        let folder = match self
            .find_child("root", "Industrialis Backups", true)
            .await?
        {
            Some(folder) => Some(folder),
            None if create => Some(self.create_folder("root", "Industrialis Backups").await?),
            None => None,
        };
        let Some(folder) = folder else {
            return Ok(None);
        };
        let path = self.paths.google_drive_state_path();
        let id = folder.id.clone();
        crate::profile_store::mutate(async move {
            let mut latest = crate::profile_store::read_object(&path)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Google Drive was disconnected"))?;
            ensure!(
                latest["refresh_token_encrypted"] == state["refresh_token_encrypted"]
                    && latest["client_id"] == state["client_id"],
                "Google Drive settings changed while finding the backup folder"
            );
            latest["root_folder_id"] = id.into();
            crate::fs_utils::write_json(&path, &latest).await
        })
        .await?;
        Ok(Some(folder.id))
    }
    async fn folder(&self, parts: &[&str], create: bool) -> anyhow::Result<Option<String>> {
        let _guard = self.folders.lock().await;
        let Some(mut parent) = self.root(create).await? else {
            return Ok(None);
        };
        for name in parts {
            parent = match self.find_child(&parent, name, true).await? {
                Some(folder) => folder.id,
                None if create => self.create_folder(&parent, name).await?.id,
                None => return Ok(None),
            };
        }
        Ok(Some(parent))
    }
    async fn object(&self, key: &str) -> anyhow::Result<Option<DriveFile>> {
        let parts = segments(key)?;
        let Some(parent) = self.folder(&parts[..parts.len() - 1], false).await? else {
            return Ok(None);
        };
        self.find_child(&parent, parts[parts.len() - 1], false)
            .await
    }
}
fn segments(key: &str) -> anyhow::Result<Vec<&str>> {
    let parts: Vec<_> = key.trim_matches('/').split('/').collect();
    ensure!(
        !parts.is_empty()
            && !parts.iter().any(|part| part.is_empty()
                || *part == "."
                || *part == ".."
                || part.contains(['\\', '\0'])),
        "Invalid backup key"
    );
    Ok(parts)
}
async fn oauth_callback(
    listener: tokio::net::TcpListener,
    redirect: &str,
    state: &str,
) -> anyhow::Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut request = Vec::new();
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            let mut byte = [0];
            while request.len() < 8192 {
                let n = stream.read(&mut byte).await?;
                if n == 0 {
                    break;
                }
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) || !request.ends_with(b"\r\n\r\n") {
            continue;
        }
        let line = String::from_utf8_lossy(&request);
        let mut parts = line.lines().next().unwrap_or_default().split_whitespace();
        if parts.next() != Some("GET") {
            let _ = stream
                .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }
        let target = parts.next().unwrap_or_default();
        if !target.starts_with('/') || target.starts_with("//") {
            continue;
        }
        let Ok(url) = reqwest::Url::parse(redirect)?.join(target) else {
            continue;
        };
        if url.path() != "/oauth2/callback" {
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        if params.get("state").map(String::as_str) != Some(state) {
            let _ = stream
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }
        if params.contains_key("error") {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            anyhow::bail!("Google authorization was cancelled or denied");
        }
        let code = params
            .get("code")
            .filter(|code| !code.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Google authorization did not return a code"))?
            .clone();
        let body = "Authorization received. Return to the launcher to check connection status.";
        let _=stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await;
        return Ok(code);
    }
}
#[async_trait::async_trait]
impl BackupStore for GoogleDrive {
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<RemoteObject>> {
        let parts = segments(prefix)?;
        let Some(parent) = self.folder(&parts, false).await? else {
            return Ok(Vec::new());
        };
        let mut pending = vec![(parent, prefix.trim_end_matches('/').to_string())];
        let mut result = Vec::new();
        while let Some((parent, path)) = pending.pop() {
            for child in self.children(&parent).await? {
                ensure!(
                    !child.name.contains(['/', '\\']) && child.name != ".." && child.name != ".",
                    "Google Drive returned an invalid backup name"
                );
                let key = format!("{path}/{}", child.name);
                if child.mime_type == FOLDER {
                    pending.push((child.id, key));
                } else {
                    result.push(RemoteObject {
                        key,
                        size_bytes: child.size.and_then(|s| s.parse().ok()),
                    });
                }
            }
        }
        Ok(result)
    }
    async fn upload(
        &self,
        key: &str,
        source: UploadSource,
        content_type: &str,
        progress: Progress,
    ) -> anyhow::Result<()> {
        let parts = segments(key)?;
        let parent = self
            .folder(&parts[..parts.len() - 1], true)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Backup folder could not be created"))?;
        let name = parts[parts.len() - 1];
        let existing = self.find_child(&parent, name, false).await?;
        let (file, bytes, total) = match source {
            UploadSource::File(path) => {
                let file = tokio::fs::File::open(path).await?;
                let total = file.metadata().await?.len();
                (Some(file), Vec::new(), total)
            }
            UploadSource::Bytes(bytes) => {
                let total = bytes.len() as u64;
                (None, bytes, total)
            }
        };
        let mut metadata = json!({"name":name,"mimeType":content_type,"appProperties":{"industrialis_backup":"true","industrialis_key":key}});
        if existing.is_none() {
            metadata["parents"] = json!([parent]);
        }
        let endpoint = if let Some(file) = &existing {
            format!("{UPLOAD}/files/{}", file.id)
        } else {
            format!("{UPLOAD}/files")
        };
        let request = if existing.is_some() {
            self.client.patch(endpoint)
        } else {
            self.client.post(endpoint)
        };
        let response = self
            .authorized(
                request
                    .query(&[
                        ("uploadType", "resumable"),
                        ("fields", "id,name,mimeType,size"),
                    ])
                    .header("X-Upload-Content-Type", content_type)
                    .header("X-Upload-Content-Length", total)
                    .json(&metadata),
            )
            .await?;
        ensure!(
            response.status().is_success(),
            "Google Drive upload could not start (HTTP {})",
            response.status().as_u16()
        );
        let location = reqwest::Url::parse(
            response
                .headers()
                .get("location")
                .context("Google Drive did not return an upload URL")?
                .to_str()?,
        )?;
        ensure!(
            trusted_upload_url(&location),
            "Google Drive returned an untrusted upload URL"
        );
        self.upload_chunks(location, file, bytes, total, progress)
            .await
    }
    async fn download(
        &self,
        key: &str,
        destination: &Path,
        progress: Progress,
    ) -> anyhow::Result<()> {
        let object = self
            .object(key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Cloud backup was not found"))?;
        let total = object.size.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let manifest = key.ends_with("/manifest.json");
        ensure!(
            !manifest || total <= 1_048_576,
            "Cloud backup manifest exceeds 1 MB"
        );
        let mut response = self
            .authorized(
                self.client
                    .get(format!("{API}/files/{}", object.id))
                    .query(&[("alt", "media")]),
            )
            .await?;
        ensure!(
            response.status().is_success(),
            "Google Drive download failed (HTTP {})",
            response.status().as_u16()
        );
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .await?;
        let mut completed = 0;
        while let Some(chunk) = response.chunk().await? {
            completed += chunk.len() as u64;
            ensure!(
                !manifest || completed <= 1_048_576,
                "Cloud backup manifest exceeds 1 MB"
            );
            output.write_all(&chunk).await?;
            progress(completed, total.max(completed));
        }
        output.sync_all().await?;
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        if let Some(object) = self.object(key).await? {
            let response = self
                .authorized(self.client.delete(format!("{API}/files/{}", object.id)))
                .await?;
            ensure!(
                response.status().is_success()
                    || response.status() == reqwest::StatusCode::NOT_FOUND,
                "Google Drive deletion failed (HTTP {})",
                response.status().as_u16()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn request(stream: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
        let mut raw = Vec::new();
        let mut buffer = [0; 16384];
        let boundary = loop {
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buffer[..n]);
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            assert!(raw.len() < 16384);
        };
        let header = String::from_utf8(raw[..boundary].to_vec()).unwrap();
        let length: usize = header
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse().unwrap())
            })
            .unwrap();
        while raw.len() < boundary + length {
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buffer[..n]);
        }
        (
            header.to_ascii_lowercase(),
            raw[boundary..boundary + length].to_vec(),
        )
    }
    #[tokio::test]
    async fn interrupted_upload_queries_the_committed_offset_and_seeks_before_retrying() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let bytes: Vec<_> = (0..(8 * 1024 * 1024 + 256 * 1024))
            .map(|i| (i % 251) as u8)
            .collect();
        let expected = bytes.clone();
        let server = tokio::spawn(async move {
            let total = expected.len();
            for (range, start, end, response) in [
                (
                    format!("bytes 0-{}/{total}", 8 * 1024 * 1024 - 1),
                    0,
                    8 * 1024 * 1024,
                    "503 Service Unavailable\r\n".to_string(),
                ),
                (
                    format!("bytes */{total}"),
                    0,
                    0,
                    "308 Resume Incomplete\r\nRange: bytes=0-131071\r\n".to_string(),
                ),
                (
                    format!("bytes 131072-{}/{total}", 131072 + 8 * 1024 * 1024 - 1),
                    131072,
                    131072 + 8 * 1024 * 1024,
                    format!(
                        "308 Resume Incomplete\r\nRange: bytes=0-{}\r\n",
                        131072 + 8 * 1024 * 1024 - 1
                    ),
                ),
                (
                    format!("bytes {}-{}/{total}", 131072 + 8 * 1024 * 1024, total - 1),
                    131072 + 8 * 1024 * 1024,
                    total,
                    "200 OK\r\n".to_string(),
                ),
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (header, body) = request(&mut stream).await;
                assert!(
                    header.contains(&format!("content-range: {range}\r\n")),
                    "{header}"
                );
                assert_eq!(body, &expected[start..end]);
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {response}Content-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let path =
            std::env::temp_dir().join(format!("industrialis-upload-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, &bytes).unwrap();
        let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = progress.clone();
        let drive = GoogleDrive::new(LauncherPaths::new(path.with_extension("profile")));
        drive
            .upload_chunks(
                format!("http://{addr}/session").parse().unwrap(),
                Some(tokio::fs::File::open(&path).await.unwrap()),
                Vec::new(),
                bytes.len() as u64,
                std::sync::Arc::new(move |done, total| output.lock().unwrap().push((done, total))),
            )
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(
            *progress.lock().unwrap(),
            vec![
                (131072, bytes.len() as u64),
                (131072 + 8 * 1024 * 1024, bytes.len() as u64),
                (bytes.len() as u64, bytes.len() as u64)
            ]
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn upload_urls_and_offsets_reject_untrusted_or_malformed_responses() {
        for url in [
            "http://www.googleapis.com/x",
            "https://googleapis.com.evil.test/x",
            "https://evil.test/x",
            "https://user@www.googleapis.com/x",
        ] {
            assert!(!trusted_upload_url(&url.parse().unwrap()));
        }
        assert!(trusted_upload_url(
            &"https://www.googleapis.com/upload/session".parse().unwrap()
        ));
        for value in [
            "bytes=0-18446744073709551615",
            "bytes=1-8",
            "garbage",
            "bytes=0-xyz",
        ] {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("range", value.parse().unwrap());
            assert!(resume_offset(&headers).is_err());
        }
    }
    #[tokio::test]
    async fn oauth_requires_the_matching_state_and_callback_and_reports_denial() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let redirect = format!("http://{addr}/oauth2/callback");
        let task =
            tokio::spawn(
                async move { oauth_callback(listener, &redirect, "expected-state").await },
            );
        for (target, status) in [
            ("/favicon.ico", "404"),
            ("/oauth2/callback?state=wrong&code=untrusted", "400"),
            (
                "/oauth2/callback?state=expected-state&code=valid%2Bcode",
                "200",
            ),
        ] {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(format!("GET {target} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).await.unwrap();
            assert!(response.starts_with(&format!("HTTP/1.1 {status}")));
        }
        assert_eq!(task.await.unwrap().unwrap(), "valid+code");
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let redirect = format!("http://{addr}/oauth2/callback");
        let task =
            tokio::spawn(
                async move { oauth_callback(listener, &redirect, "expected-state").await },
            );
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GET /oauth2/callback?state=expected-state&error=access_denied HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 400"));
        assert!(task
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("denied"));
    }
    #[tokio::test]
    async fn credential_failure_keeps_the_original_provider_state() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-drive-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(paths.base()).unwrap();
        let raw=br#"{"schema_version":1,"client_id":"existing.apps.googleusercontent.com","refresh_token_encrypted":"invalid","root_folder_id":"keep","future":{"keep":true}}"#;
        std::fs::write(paths.google_drive_state_path(), raw).unwrap();
        let drive = GoogleDrive::new(paths.clone());
        let status = drive.status().await.unwrap();
        assert!(status.configured);
        assert!(!status.connected);
        assert!(status.error.is_some());
        assert_eq!(std::fs::read(paths.google_drive_state_path()).unwrap(), raw);
        drive.disconnect().await.unwrap();
        let state: Value =
            serde_json::from_slice(&std::fs::read(paths.google_drive_state_path()).unwrap())
                .unwrap();
        assert_eq!(state["future"], json!({"keep":true}));
        assert_eq!(state["client_id"], "existing.apps.googleusercontent.com");
        assert!(state.get("refresh_token_encrypted").is_none());
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
