//! The operating system's credential store for the Commons login, the way
//! `immich/secrets.rs` does it for the Immich key (kept separate on purpose,
//! so upstream changes to that file never collide with this one).

const SERVICE: &str = "io.github.CyberTimon.RapidRAW.commons";
const ACCOUNT: &str = "oauth-login";

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
))]
mod store {
    use std::sync::OnceLock;

    fn init() -> bool {
        static READY: OnceLock<bool> = OnceLock::new();
        *READY.get_or_init(|| {
            #[cfg(target_os = "linux")]
            let store = zbus_secret_service_keyring_store::Store::new();
            #[cfg(target_os = "macos")]
            let store = apple_native_keyring_store::keychain::Store::new();
            #[cfg(target_os = "windows")]
            let store = windows_native_keyring_store::Store::new();
            #[cfg(target_os = "android")]
            let store = android_native_keyring_store::Store::new();
            match store {
                Ok(store) => {
                    keyring_core::set_default_store(store);
                    true
                }
                Err(e) => {
                    log::info!(
                        "No credential store available, keeping the Commons login in a file: {e}"
                    );
                    false
                }
            }
        })
    }

    pub fn with_entry<T: Send + 'static>(
        f: impl FnOnce(&keyring_core::Entry) -> keyring_core::Result<T> + Send + 'static,
    ) -> Option<keyring_core::Result<T>> {
        std::thread::spawn(move || {
            if !init() {
                return None;
            }
            Some(keyring_core::Entry::new(super::SERVICE, super::ACCOUNT).and_then(|e| f(&e)))
        })
        .join()
        .ok()
        .flatten()
    }
}

/// Store `secret`; `false` if there is no credential store or it refused.
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
))]
pub fn save(secret: &str) -> bool {
    let secret = secret.to_string();
    match store::with_entry(move |entry| entry.set_password(&secret)) {
        Some(Ok(())) => true,
        Some(Err(e)) => {
            log::warn!("Could not store the Commons login in the credential store: {e}");
            false
        }
        None => false,
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
))]
pub fn load() -> Option<String> {
    match store::with_entry(|entry| entry.get_password())? {
        Ok(secret) => Some(secret),
        Err(keyring_core::Error::NoEntry) => None,
        Err(e) => {
            log::warn!("Could not read the Commons login from the credential store: {e}");
            None
        }
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
))]
pub fn clear() {
    let result = store::with_entry(|entry| match entry.delete_credential() {
        Err(keyring_core::Error::NoEntry) => Ok(()),
        other => other,
    });
    if let Some(Err(e)) = result {
        log::warn!("Could not remove the Commons login from the credential store: {e}");
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
)))]
pub fn save(_secret: &str) -> bool {
    false
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
)))]
pub fn load() -> Option<String> {
    None
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "android"
)))]
pub fn clear() {}
