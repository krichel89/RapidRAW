//! The signed-in client, shared by the export and the commands, and the places
//! the Commons files live in.

use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager};

use super::client::{CommonsClient, Persist};
use super::login_store;
use super::oauth::{StoredLogin, Tokens};
use super::settings::{self, CommonsSettings};

/// The client together with the client id it was made for.
type Cached = Option<(String, Arc<CommonsClient>)>;

static CLIENT: Lazy<RwLock<Cached>> = Lazy::new(|| RwLock::new(None));

fn data_file(app_handle: &AppHandle, name: &str) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map(|dir| dir.join(name))
        .map_err(|e| e.to_string())
}

pub fn settings_file(app_handle: &AppHandle) -> Result<PathBuf, String> {
    data_file(app_handle, "commons.json")
}

pub fn login_file(app_handle: &AppHandle) -> Result<PathBuf, String> {
    data_file(app_handle, "commons-login")
}

pub fn load_settings(app_handle: &AppHandle) -> CommonsSettings {
    settings_file(app_handle)
        .map(|file| settings::load_from(&file))
        .unwrap_or_default()
}

/// Keeps every new token set: the server rotates the refresh token, so the
/// one from the last refresh is the only valid one.
pub fn persister(app_handle: &AppHandle) -> Persist {
    let file = login_file(app_handle);
    Arc::new(move |tokens: &Tokens| {
        let Ok(file) = &file else { return };
        let login = StoredLogin {
            refresh_token: tokens.refresh_token.clone(),
            username: tokens.username.clone(),
        };
        if let Err(e) = login_store::save(file, &login) {
            log::warn!("Could not keep the renewed Commons login: {e}");
        }
    })
}

/// The client for the stored login. Errors say what to do.
pub fn client(app_handle: &AppHandle) -> Result<Arc<CommonsClient>, String> {
    let settings = load_settings(app_handle);
    if settings.client_id.is_empty() {
        return Err(
            "Commons is not set up yet. Enter the OAuth client ID in the export settings."
                .to_string(),
        );
    }
    if let Some((id, client)) = CLIENT.read().unwrap().as_ref()
        && *id == settings.client_id
    {
        return Ok(client.clone());
    }
    let loaded = login_store::load(&login_file(app_handle)?)
        .ok_or("Not signed in to Commons. Sign in in the export settings.")?;
    let tokens = Tokens {
        refresh_token: loaded.login.refresh_token,
        username: loaded.login.username,
        ..Default::default()
    };
    let client = Arc::new(CommonsClient::new(
        &settings.client_id,
        tokens,
        persister(app_handle),
    )?);
    *CLIENT.write().unwrap() = Some((settings.client_id, client.clone()));
    Ok(client)
}

/// Forget the cached client (after signing in or out, or a new client id).
pub fn reset() {
    *CLIENT.write().unwrap() = None;
}
