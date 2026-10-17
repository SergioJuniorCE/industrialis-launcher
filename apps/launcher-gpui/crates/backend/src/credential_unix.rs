//! Chromium's legacy CBC envelope. Platform key lookup never resets an old key.
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use anyhow::{ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine as _};
#[cfg(any(target_os = "macos", target_os = "linux"))]
const APPS: [&str; 3] = [
    "Industrialis Launcher",
    "@industrialis/launcher",
    "industrialis-launcher",
];
fn key(password: &[u8], iterations: u32) -> [u8; 16] {
    let mut key = [0; 16];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA1,
        std::num::NonZeroU32::new(iterations).unwrap(),
        b"saltysalt",
        password,
        &mut key,
    );
    key
}
fn open(raw: &[u8], password: &[u8], iterations: u32) -> anyhow::Result<String> {
    ensure!(
        raw.len() > 3 && (raw.len() - 3) % 16 == 0,
        "Encrypted credential is incomplete"
    );
    let plain =
        cbc::Decryptor::<aes::Aes128>::new(&key(password, iterations).into(), &[32; 16].into())
            .decrypt_padded_vec_mut::<Pkcs7>(&raw[3..])
            .map_err(|_| {
                anyhow::anyhow!("The existing credential key could not decrypt Google Drive")
            })?;
    String::from_utf8(plain).context("Credential is not valid UTF-8")
}
fn seal(value: &str, password: &[u8], iterations: u32, prefix: &[u8]) -> String {
    let mut raw = prefix.to_vec();
    raw.extend(
        cbc::Encryptor::<aes::Aes128>::new(&key(password, iterations).into(), &[32; 16].into())
            .encrypt_padded_vec_mut::<Pkcs7>(value.as_bytes()),
    );
    STANDARD.encode(raw)
}

#[cfg(target_os = "macos")]
fn mac_password(app: &str, create: bool) -> anyhow::Result<Option<Vec<u8>>> {
    use std::ffi::c_void;
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecKeychainFindGenericPassword(
            keychain: *const c_void,
            service_len: u32,
            service: *const u8,
            account_len: u32,
            account: *const u8,
            password_len: *mut u32,
            password: *mut *mut c_void,
            item: *mut *mut c_void,
        ) -> i32;
        fn SecKeychainItemFreeContent(attributes: *mut c_void, password: *mut c_void) -> i32;
        fn SecKeychainAddGenericPassword(
            keychain: *const c_void,
            service_len: u32,
            service: *const u8,
            account_len: u32,
            account: *const u8,
            password_len: u32,
            password: *const u8,
            item: *mut *mut c_void,
        ) -> i32;
    }
    let service = format!("{app} Safe Storage");
    let mut len = 0;
    let mut data = std::ptr::null_mut();
    let status = unsafe {
        SecKeychainFindGenericPassword(
            std::ptr::null(),
            service.len() as u32,
            service.as_ptr(),
            app.len() as u32,
            app.as_ptr(),
            &mut len,
            &mut data,
            std::ptr::null_mut(),
        )
    };
    if status == 0 {
        let secret =
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize).to_vec() };
        unsafe {
            SecKeychainItemFreeContent(std::ptr::null_mut(), data);
        }
        ensure!(
            !secret.is_empty(),
            "Existing Keychain password is empty; it has been kept"
        );
        return Ok(Some(secret));
    }
    ensure!(status==-25300,"The launcher Keychain item is locked or access was denied (status {status}); it has been kept");
    if !create {
        return Ok(None);
    }
    use ring::rand::SecureRandom;
    let mut random = [0; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| anyhow::anyhow!("Secure random generation failed"))?;
    let password = STANDARD.encode(random).into_bytes();
    let status = unsafe {
        SecKeychainAddGenericPassword(
            std::ptr::null(),
            service.len() as u32,
            service.as_ptr(),
            app.len() as u32,
            app.as_ptr(),
            password.len() as u32,
            password.as_ptr(),
            std::ptr::null_mut(),
        )
    };
    ensure!(
        status == 0,
        "Could not store the launcher Keychain key (status {status})"
    );
    Ok(Some(password))
}

#[cfg(target_os = "linux")]
fn libsecret_libraries() -> anyhow::Result<&'static (libloading::Library, libloading::Library)> {
    use libloading::Library;
    use std::sync::OnceLock;

    // libsecret registers GLib types and starts native workers that outlive a
    // synchronous lookup. Unloading its code or static strings between calls
    // leaves those process-wide registrations pointing into unmapped memory.
    static LIBRARIES: OnceLock<(Library, Library)> = OnceLock::new();
    if LIBRARIES.get().is_none() {
        let libraries = unsafe {
            (
                Library::new("libsecret-1.so.0")
                    .context("The Secret Service credential store is unavailable")?,
                Library::new("libglib-2.0.so.0")?,
            )
        };
        // A concurrent caller may have initialized it first. Its retained
        // handles keep the same loaded libraries alive when ours are dropped.
        let _ = LIBRARIES.set(libraries);
    }
    Ok(LIBRARIES.get().expect("Credential libraries initialized"))
}

#[cfg(target_os = "linux")]
fn libsecret_password(app: &str, create: bool) -> anyhow::Result<Option<Vec<u8>>> {
    use libloading::Symbol;
    use std::ffi::{c_char, c_void, CStr, CString};
    type SchemaNew = unsafe extern "C" fn(*const c_char, i32, ...) -> *mut c_void;
    type Unref = unsafe extern "C" fn(*mut c_void);
    type Lookup =
        unsafe extern "C" fn(*const c_void, *mut c_void, *mut *mut c_void, ...) -> *mut c_char;
    type Store = unsafe extern "C" fn(
        *const c_void,
        *const c_char,
        *const c_char,
        *const c_char,
        *mut c_void,
        *mut *mut c_void,
        ...
    ) -> i32;
    unsafe {
        let (lib, glib) = libsecret_libraries()?;
        let make: Symbol<SchemaNew> = lib.get(b"secret_schema_new\0")?;
        let unref: Symbol<Unref> = lib.get(b"secret_schema_unref\0")?;
        let lookup: Symbol<Lookup> = lib.get(b"secret_password_lookup_sync\0")?;
        let free: Symbol<unsafe extern "C" fn(*mut c_char)> = lib.get(b"secret_password_free\0")?;
        let error_free: Symbol<Unref> = glib.get(b"g_error_free\0")?;
        let schema = make(
            c"chrome_libsecret_os_crypt_password_v2".as_ptr(),
            1,
            c"application".as_ptr(),
            0i32,
            std::ptr::null::<c_char>(),
        );
        ensure!(
            !schema.is_null(),
            "Could not initialize the credential schema"
        );
        let application = CString::new(app)?;
        let mut error = std::ptr::null_mut();
        let secret = lookup(
            schema,
            std::ptr::null_mut(),
            &mut error,
            c"application".as_ptr(),
            application.as_ptr(),
            std::ptr::null::<c_char>(),
        );
        if !error.is_null() {
            error_free(error);
            if !secret.is_null() {
                free(secret);
            }
            unref(schema);
            anyhow::bail!("Secret Service is locked or credential access was denied; the original key has been kept");
        }
        if !secret.is_null() {
            let result = CStr::from_ptr(secret).to_bytes().to_vec();
            free(secret);
            unref(schema);
            ensure!(
                !result.is_empty(),
                "Existing Secret Service key is empty; it has been kept"
            );
            return Ok(Some(result));
        }
        if !create {
            unref(schema);
            return Ok(None);
        }
        use ring::rand::SecureRandom;
        let mut random = [0; 16];
        ring::rand::SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| anyhow::anyhow!("Secure random generation failed"))?;
        let password = CString::new(STANDARD.encode(random))?;
        let store: Symbol<Store> = lib.get(b"secret_password_store_sync\0")?;
        let success = store(
            schema,
            std::ptr::null(),
            c"Chromium Safe Storage".as_ptr(),
            password.as_ptr(),
            std::ptr::null_mut(),
            &mut error,
            c"application".as_ptr(),
            application.as_ptr(),
            std::ptr::null::<c_char>(),
        );
        unref(schema);
        if !error.is_null() {
            error_free(error);
        }
        ensure!(
            success != 0,
            "Could not save the launcher key in Secret Service"
        );
        Ok(Some(password.as_bytes().to_vec()))
    }
}
#[cfg(target_os = "linux")]
fn kwallet_password(app: &str, create: bool) -> anyhow::Result<Option<Vec<u8>>> {
    let connection = zbus::blocking::Connection::session()?;
    for version in ["6", "5", ""] {
        let service = format!("org.kde.kwalletd{version}");
        let path = format!("/modules/kwalletd{version}");
        let Ok(proxy) = zbus::blocking::Proxy::new(
            &connection,
            service.as_str(),
            path.as_str(),
            "org.kde.KWallet",
        ) else {
            continue;
        };
        let Ok(wallet): Result<String, _> = proxy.call("networkWallet", &()) else {
            continue;
        };
        let handle: i32 = proxy.call("open", &(wallet, 0i64, app))?;
        ensure!(
            handle >= 0,
            "KWallet access was denied; the original key has been kept"
        );
        let result = (|| {
            for (folder, key) in [
                ("Chromium Keys", "Chromium Safe Storage"),
                ("Chrome Keys", "Chrome Safe Storage"),
            ] {
                let exists: bool = proxy.call("hasEntry", &(handle, folder, key, app))?;
                if exists {
                    let password: String =
                        proxy.call("readPassword", &(handle, folder, key, app))?;
                    ensure!(
                        !password.is_empty(),
                        "Existing KWallet key is empty; it has been kept"
                    );
                    return Ok(Some(password.into_bytes()));
                }
            }
            if !create { return Ok(None); }
            let folder="Chromium Keys";
            let exists:bool=proxy.call("hasFolder", &(handle,folder,app))?;
            if !exists {
                let made:bool=proxy.call("createFolder", &(handle,folder,app))?;
                ensure!(made,"Could not create the launcher credential folder in KWallet");
            }
            use ring::rand::SecureRandom;
            let mut random=[0;16];
            ring::rand::SystemRandom::new().fill(&mut random).map_err(|_|anyhow::anyhow!("Secure random generation failed"))?;
            let password=STANDARD.encode(random);
            // The profile owner excludes other launcher writers. Recheck before
            // creating an item so an existing OS credential is never reset.
            let exists:bool=proxy.call("hasEntry", &(handle,folder,"Chromium Safe Storage",app))?;
            ensure!(!exists,"A credential was created concurrently; retry to reuse it");
            let status:i32=proxy.call("writePassword", &(handle,folder,"Chromium Safe Storage",password.as_str(),app))?;
            ensure!(status==0,"Could not save the launcher key in KWallet");
            Ok::<_, anyhow::Error>(Some(password.into_bytes()))
        })();
        let _: Result<i32, _> = proxy.call("close", &(handle, false, app));
        return result;
    }
    Ok(None)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn decrypt(value: &str) -> anyhow::Result<String> {
    let raw = STANDARD
        .decode(value)
        .context("Encrypted Google Drive credential is invalid")?;
    #[cfg(target_os = "linux")]
    if raw.starts_with(b"v10") {
        return open(&raw, b"peanuts", 1);
    }
    #[cfg(target_os = "macos")]
    let (prefix, iterations) = (b"v10", 1003);
    #[cfg(target_os = "linux")]
    let (prefix, iterations) = (b"v11", 1);
    ensure!(
        raw.starts_with(prefix),
        "The existing credential format is unsupported; it has been kept"
    );
    let mut error = None;
    for app in APPS {
        #[cfg(target_os = "macos")]
        let candidates = vec![mac_password(app, false)];
        #[cfg(target_os = "linux")]
        let lookups: [fn(&str,bool)->anyhow::Result<Option<Vec<u8>>>;2]=[libsecret_password,kwallet_password];
        let candidates=lookups.into_iter().map(|lookup|lookup(app,false));
        for candidate in candidates {
            match candidate {
                Ok(Some(password)) => match open(&raw, &password, iterations) {
                    Ok(value) => return Ok(value),
                    Err(e) => error = Some(e),
                },
                Ok(None) => {}
                Err(e) => error = Some(e),
            }
        }
    }
    Err(error.unwrap_or_else(||anyhow::anyhow!("The original launcher credential key was not found; existing credentials have been kept")))
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn encrypt(value: &str, existing: Option<&str>) -> anyhow::Result<String> {
    if let Some(existing) = existing {
        let raw = STANDARD.decode(existing)?;
        #[cfg(target_os = "macos")]
        let iterations = 1003;
        #[cfg(target_os = "linux")]
        let iterations = 1;
        for app in APPS {
            #[cfg(target_os = "macos")]
            let candidates = vec![mac_password(app, false)];
            #[cfg(target_os = "linux")]
            let lookups: [fn(&str,bool)->anyhow::Result<Option<Vec<u8>>>;2]=[libsecret_password,kwallet_password];
        let candidates=lookups.into_iter().map(|lookup|lookup(app,false));
            for candidate in candidates {
                if let Ok(Some(password)) = candidate {
                    if open(&raw, &password, iterations).is_ok() {
                        return Ok(seal(value, &password, iterations, &raw[..3]));
                    }
                }
            }
        }
        anyhow::bail!("The existing credential key is unavailable; it has been kept. Unlock the original OS credential store and retry.");
    }
    // Prefer the original app's existing key. Creation is only allowed when no
    // encrypted provider credential exists, and never replaces a missing key.
    #[cfg(target_os = "macos")]
    {
        for app in APPS {
            if let Some(password) = mac_password(app, false)? {
                return Ok(seal(value, &password, 1003, b"v10"));
            }
        }
        let password = mac_password(APPS[0], true)?.context("Keychain is unavailable")?;
        Ok(seal(value, &password, 1003, b"v10"))
    }
    #[cfg(target_os = "linux")]
    {
        for app in APPS {
            if let Ok(Some(password)) = libsecret_password(app, false) {
                return Ok(seal(value, &password, 1, b"v11"));
            }
            if let Some(password) = kwallet_password(app, false).unwrap_or(None) {
                return Ok(seal(value, &password, 1, b"v11"));
            }
        }
        let password=match libsecret_password(APPS[0],true){Ok(Some(secret))=>secret,secret_result=>kwallet_password(APPS[0],true)?.or_else(||secret_result.ok().flatten()).context("Secure credential storage is unavailable")?};
        Ok(seal(value, &password, 1, b"v11"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chromium_cbc_matches_independent_node_crypto_vectors() {
        for (password, iterations, prefix, expected) in [
            (
                b"synthetic-keychain-secret".as_slice(),
                1003,
                b"v10".as_slice(),
                "djEw57EUbBSPgkNSpsG/t2NDDKFSbJIFf7VHrSmNyCRjqow=",
            ),
            (
                b"synthetic-linux-secret".as_slice(),
                1,
                b"v11".as_slice(),
                "djExtFcVIJuaOZztzBIvT6G2zEkAyfiwZMWhZkcHVUbtkp8=",
            ),
        ] {
            let encrypted = seal("synthetic refresh token", password, iterations, prefix);
            assert_eq!(encrypted, expected);
            let raw = STANDARD.decode(&encrypted).unwrap();
            assert_eq!(
                open(&raw, password, iterations).unwrap(),
                "synthetic refresh token"
            );
            assert!(open(&raw, b"wrong key", iterations).is_err());
        }
    }
}
