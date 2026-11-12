//! Where the signed-in user's refresh token is kept between sessions: the
//! operating system's credential store when there is one, otherwise a file
//! that only the user can read (the same arrangement as the Immich key).

use super::keyring;
use super::oauth::StoredLogin;
use std::path::Path;

pub struct Loaded {
    pub login: StoredLogin,
    pub in_credential_store: bool,
}

/// Keep `login`. Returns whether it went into the credential store.
pub fn save(file: &Path, login: &StoredLogin) -> Result<bool, String> {
    let text = serde_json::to_string(login).map_err(|e| e.to_string())?;
    if keyring::save(&text) {
        let _ = std::fs::remove_file(file);
        return Ok(true);
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, text).map_err(|e| e.to_string())?;
    restrict_permissions(file);
    Ok(false)
}

pub fn load(file: &Path) -> Option<Loaded> {
    let parse = |text: String| {
        serde_json::from_str::<StoredLogin>(&text)
            .ok()
            .filter(|login| !login.refresh_token.is_empty())
    };
    if let Some(login) = keyring::load().and_then(parse) {
        return Some(Loaded {
            login,
            in_credential_store: true,
        });
    }
    std::fs::read_to_string(file)
        .ok()
        .and_then(parse)
        .map(|login| Loaded {
            login,
            in_credential_store: false,
        })
}

pub fn clear(file: &Path) {
    keyring::clear();
    let _ = std::fs::remove_file(file);
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    // The credential store itself is not exercised here (no desktop session
    // in a test run); the file fallback is.
    #[test]
    fn file_fallback_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("commons-login");
        assert!(load(&file).is_none());
        let login = StoredLogin {
            refresh_token: "r1".into(),
            username: "Harald Krichel".into(),
        };
        std::fs::write(&file, serde_json::to_string(&login).unwrap()).unwrap();
        let loaded = load(&file).unwrap();
        assert_eq!(loaded.login, login);
        assert!(!loaded.in_credential_store);

        clear(&file);
        assert!(!file.exists());
    }

    #[test]
    fn a_login_without_refresh_token_is_no_login() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("commons-login");
        std::fs::write(&file, r#"{"username":"x"}"#).unwrap();
        assert!(load(&file).is_none());
        std::fs::write(&file, "garbage").unwrap();
        assert!(load(&file).is_none());
    }
}
