//! Compatibility with Electron's synchronous safeStorage envelopes. Never
//! replace a missing/corrupt existing key to make a decryption failure disappear.
use crate::paths::LauncherPaths;
#[cfg(windows)]
use anyhow::{ensure, Context};
#[cfg(windows)]
use base64::{engine::general_purpose::STANDARD, Engine as _};
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
fn local_state_candidates(paths: &LauncherPaths) -> Vec<PathBuf> {
    let mut result = vec![paths.base().join("Local State")];
    // Electron userData and the launcher backend's profile directory are distinct
    // in older builds. Read the original key where Electron put it, in place.
    if paths.base() == crate::paths::system_data_dir()
        && std::env::var_os("INDUSTRIALIS_DATA_DIR").is_none()
    {
        if let Some(base) = directories::BaseDirs::new() {
            for app in [
                "Industrialis Launcher",
                "@industrialis/launcher",
                "industrialis-launcher",
            ] {
                let p = base.config_dir().join(app).join("Local State");
                if !result.contains(&p) {
                    result.push(p);
                }
            }
        }
    }
    result
}
#[cfg(windows)]
fn key_from_state(path: &std::path::Path) -> anyhow::Result<Option<Vec<u8>>> {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let state: serde_json::Value = serde_json::from_slice(&raw)
        .context("The original Electron Local State is invalid; it has been kept")?;
    let Some(key) = state
        .pointer("/os_crypt/encrypted_key")
        .and_then(|v| v.as_str())
    else {
        return Ok(None);
    };
    let raw = STANDARD
        .decode(key)
        .context("The Electron credential key is invalid")?;
    ensure!(
        raw.starts_with(b"DPAPI"),
        "The Electron credential key uses an unsupported encryption format"
    );
    let key = dpapi(&raw[5..], false)?;
    ensure!(
        key.len() == 32,
        "The Electron credential key has an invalid length"
    );
    Ok(Some(key))
}
#[cfg(windows)]
fn dpapi(raw: &[u8], encrypt: bool) -> anyhow::Result<Vec<u8>> {
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut std::ffi::c_void,
            prompt: *mut std::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut std::ffi::c_void,
            prompt: *mut std::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    ensure!(raw.len() <= u32::MAX as usize, "Credential is too large");
    let input = Blob {
        size: raw.len() as u32,
        data: raw.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        }
    };
    ensure!(
        success != 0,
        "Windows credential storage is unavailable for this user: {}",
        std::io::Error::last_os_error()
    );
    let result = unsafe { std::slice::from_raw_parts(output.data, output.size as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(output.data, 0, output.size as usize);
        LocalFree(output.data.cast());
    }
    Ok(result)
}
#[cfg(windows)]
pub fn decrypt(paths: &LauncherPaths, value: &str) -> anyhow::Result<String> {
    let raw = STANDARD
        .decode(value)
        .context("Encrypted Google Drive credential is invalid")?;
    if !raw.starts_with(b"v10") {
        return String::from_utf8(dpapi(&raw, false)?).context("Credential is not valid UTF-8");
    }
    ensure!(
        raw.len() >= 3 + 12 + 16,
        "Encrypted Google Drive credential is incomplete"
    );
    // Try only our known old app directories. A failed candidate never causes a
    // rewrite, deletion, plaintext fallback, or a forced reconnect.
    let mut last_error = None;
    for candidate in local_state_candidates(paths) {
        match key_from_state(&candidate) {
            Ok(Some(key)) => {
                let key = ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, &key)
                    .map_err(|_| anyhow::anyhow!("Credential key is invalid"))?;
                let key = ring::aead::LessSafeKey::new(key);
                let mut ciphertext = raw[15..].to_vec();
                if let Ok(text) = key.open_in_place(
                    ring::aead::Nonce::try_assume_unique_for_key(&raw[3..15]).unwrap(),
                    ring::aead::Aad::empty(),
                    &mut ciphertext,
                ) {
                    return String::from_utf8(text.to_vec())
                        .context("Credential is not valid UTF-8");
                }
            }
            Ok(None) => {}
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(||anyhow::anyhow!("The original Electron credential key could not decrypt Google Drive. Existing credentials have been kept.")))
}
#[cfg(windows)]
pub async fn encrypt(paths: &LauncherPaths, value: String) -> anyhow::Result<String> {
    use ring::rand::SecureRandom;
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        tokio::task::spawn_blocking(move || {
            let mut existing = None;
            for candidate in local_state_candidates(&paths) {
                if let Some(key) = key_from_state(&candidate)? {
                    existing = Some(key);
                    break;
                }
            }
            let key = if let Some(key) = existing {
                key
            } else {
                let state_path = paths.base().join("Local State");
                let mut state = match std::fs::read(&state_path) {
                    Ok(raw) => serde_json::from_slice::<serde_json::Value>(&raw)?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                    Err(e) => return Err(e.into()),
                };
                ensure!(
                    state.is_object(),
                    "Electron Local State is invalid; it has been kept"
                );
                let mut key = vec![0; 32];
                ring::rand::SystemRandom::new()
                    .fill(&mut key)
                    .map_err(|_| anyhow::anyhow!("Secure random generation failed"))?;
                let mut protected = b"DPAPI".to_vec();
                protected.extend(dpapi(&key, true)?);
                let object = state
                    .as_object_mut()
                    .unwrap()
                    .entry("os_crypt")
                    .or_insert_with(|| serde_json::json!({}));
                ensure!(
                    object.is_object(),
                    "Electron credential settings are invalid; they have been kept"
                );
                object["encrypted_key"] = STANDARD.encode(protected).into();
                crate::fs_utils::write_atomic(
                    &state_path,
                    serde_json::to_string_pretty(&state)?.as_bytes(),
                )?;
                key
            };
            let key = ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, &key)
                .map_err(|_| anyhow::anyhow!("Credential key is invalid"))?;
            let key = ring::aead::LessSafeKey::new(key);
            let mut nonce = [0; 12];
            ring::rand::SystemRandom::new()
                .fill(&mut nonce)
                .map_err(|_| anyhow::anyhow!("Secure random generation failed"))?;
            let mut ciphertext = value.into_bytes();
            key.seal_in_place_append_tag(
                ring::aead::Nonce::assume_unique_for_key(nonce),
                ring::aead::Aad::empty(),
                &mut ciphertext,
            )
            .map_err(|_| anyhow::anyhow!("Credential encryption failed"))?;
            let mut envelope = b"v10".to_vec();
            envelope.extend(nonce);
            envelope.extend(ciphertext);
            Ok(STANDARD.encode(envelope))
        })
        .await?
    })
    .await
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn decrypt(_paths: &LauncherPaths, value: &str) -> anyhow::Result<String> {
    crate::credential_unix::decrypt(value)
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub async fn encrypt(paths: &LauncherPaths, value: String) -> anyhow::Result<String> {
    let paths = paths.clone();
    crate::profile_store::mutate(async move {
        let state = crate::profile_store::read_object(&paths.google_drive_state_path()).await?;
        let existing = state
            .as_ref()
            .and_then(|s| s["refresh_token_encrypted"].as_str())
            .map(str::to_string);
        tokio::task::spawn_blocking(move || {
            crate::credential_unix::encrypt(&value, existing.as_deref())
        })
        .await?
    })
    .await
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn decrypt(_paths: &LauncherPaths, _value: &str) -> anyhow::Result<String> {
    anyhow::bail!("Native credential storage is unsupported on this platform; existing credentials have been kept")
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub async fn encrypt(_paths: &LauncherPaths, _value: String) -> anyhow::Result<String> {
    anyhow::bail!("Native credential storage is unsupported on this platform")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn existing_key_and_envelope_roundtrip_without_rewriting_local_state() {
        let paths = LauncherPaths::new(
            std::env::temp_dir().join(format!("industrialis-credential-{}", uuid::Uuid::new_v4())),
        );
        let encrypted = encrypt(&paths, "synthetic-refresh-token".into())
            .await
            .unwrap();
        assert_eq!(
            decrypt(&paths, &encrypted).unwrap(),
            "synthetic-refresh-token"
        );
        let state = std::fs::read(paths.base().join("Local State")).unwrap();
        let second = encrypt(&paths, "second synthetic value".into())
            .await
            .unwrap();
        assert_eq!(decrypt(&paths, &second).unwrap(), "second synthetic value");
        assert_eq!(
            std::fs::read(paths.base().join("Local State")).unwrap(),
            state
        );
        let mut bad = STANDARD.decode(encrypted).unwrap();
        bad[16] ^= 1;
        assert!(decrypt(&paths, &STANDARD.encode(bad)).is_err());
        assert_eq!(
            std::fs::read(paths.base().join("Local State")).unwrap(),
            state
        );
        std::fs::write(paths.base().join("Local State"), b"broken").unwrap();
        assert!(encrypt(&paths, "do not erase".into()).await.is_err());
        assert_eq!(
            std::fs::read(paths.base().join("Local State")).unwrap(),
            b"broken"
        );
        std::fs::remove_dir_all(paths.base()).unwrap();
    }
}
