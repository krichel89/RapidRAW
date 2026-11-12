use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};

use super::client::{self, CommonsClient, Endpoints};
use super::oauth::{self, Redirect, StoredLogin, Tokens};
use super::settings::CommonsSettings;
use super::{export, login_store, session, settings};

/// How long a sign-in may wait for the browser.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonsStatus {
    /// A client id has been entered.
    pub configured: bool,
    pub logged_in: bool,
    pub username: String,
    pub in_credential_store: bool,
    /// The callback the OAuth client must be registered with.
    pub redirect_uri: &'static str,
}

fn status(app_handle: &AppHandle) -> CommonsStatus {
    let configured = !session::load_settings(app_handle).client_id.is_empty();
    let loaded = session::login_file(app_handle)
        .ok()
        .and_then(|file| login_store::load(&file));
    CommonsStatus {
        configured,
        logged_in: configured && loaded.is_some(),
        username: loaded
            .as_ref()
            .map(|l| l.login.username.clone())
            .unwrap_or_default(),
        in_credential_store: loaded.as_ref().is_some_and(|l| l.in_credential_store),
        redirect_uri: oauth::REDIRECT_URI,
    }
}

#[tauri::command]
pub fn commons_get_settings(app_handle: AppHandle) -> CommonsSettings {
    session::load_settings(&app_handle)
}

/// Check and keep the settings. A different client id ends the current login,
/// because the tokens belong to the old client.
#[tauri::command]
pub fn commons_set_settings(
    mut new_settings: CommonsSettings,
    app_handle: AppHandle,
) -> Result<CommonsSettings, String> {
    new_settings
        .normalize()
        .map_err(|problems| problems.join("; "))?;
    let file = session::settings_file(&app_handle)?;
    let old = settings::load_from(&file);
    settings::save_to(&file, &new_settings)?;
    if old.client_id != new_settings.client_id {
        login_store::clear(&session::login_file(&app_handle)?);
    }
    session::reset();
    Ok(new_settings)
}

#[tauri::command]
pub fn commons_status(app_handle: AppHandle) -> CommonsStatus {
    status(&app_handle)
}

// ── Sign-in ──────────────────────────────────────────────────────────────

struct Pending {
    verifier: String,
    state: String,
    client_id: String,
    codes: mpsc::Sender<Result<String, String>>,
    receiver: Option<mpsc::Receiver<Result<String, String>>>,
    /// Ends the local listener.
    stop: Option<oneshot::Sender<()>>,
}

static PENDING: Lazy<Mutex<Option<Pending>>> = Lazy::new(|| Mutex::new(None));

fn cancel_pending() {
    if let Some(mut pending) = PENDING.lock().unwrap().take()
        && let Some(stop) = pending.stop.take()
    {
        let _ = stop.send(());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    /// Open this in the browser.
    pub url: String,
    /// Whether the answer will arrive by itself. If not (the port is taken),
    /// the user pastes the code from the browser's address bar.
    pub loopback: bool,
}

/// Begin the sign-in: returns the address for the browser. The answer is
/// awaited by [`commons_complete_login`].
#[tauri::command]
pub async fn commons_begin_login(app_handle: AppHandle) -> Result<LoginStart, String> {
    let settings = session::load_settings(&app_handle);
    if settings.client_id.is_empty() {
        return Err("Enter the OAuth client ID first.".to_string());
    }
    cancel_pending();

    let pkce = oauth::new_pkce();
    let state = oauth::new_state();
    let (codes, receiver) = mpsc::channel(8);
    let (stop, stop_receiver) = oneshot::channel();

    let loopback = match TcpListener::bind(("127.0.0.1", oauth::LOOPBACK_PORT)).await {
        Ok(listener) => {
            tauri::async_runtime::spawn(listen(
                listener,
                state.clone(),
                codes.clone(),
                stop_receiver,
            ));
            true
        }
        Err(e) => {
            log::warn!(
                "Port {} is not available for the Commons sign-in: {e}",
                oauth::LOOPBACK_PORT
            );
            false
        }
    };

    let url = oauth::authorize_url(&settings.client_id, &state, &pkce.challenge);
    *PENDING.lock().unwrap() = Some(Pending {
        verifier: pkce.verifier,
        state,
        client_id: settings.client_id,
        codes,
        receiver: Some(receiver),
        stop: Some(stop),
    });
    Ok(LoginStart { url, loopback })
}

/// Hand over a code (or the whole address) the user copied from the browser,
/// for when the automatic catch did not work.
#[tauri::command]
pub fn commons_submit_code(text: String) -> Result<(), String> {
    let guard = PENDING.lock().unwrap();
    let pending = guard.as_ref().ok_or("No sign-in is running.")?;
    if let Some(state) = oauth::state_from_input(&text)
        && state != pending.state
    {
        return Err("That address belongs to a different sign-in.".to_string());
    }
    let code = oauth::code_from_input(&text).ok_or("No code found in that text.")?;
    pending
        .codes
        .try_send(Ok(code))
        .map_err(|_| "The sign-in is no longer waiting.".to_string())
}

#[tauri::command]
pub fn commons_cancel_login() {
    cancel_pending();
}

/// Wait for the browser's answer, then trade the code for tokens, ask Commons
/// who signed in, and keep the login.
#[tauri::command]
pub async fn commons_complete_login(app_handle: AppHandle) -> Result<CommonsStatus, String> {
    let (mut receiver, verifier, client_id) = {
        let mut guard = PENDING.lock().unwrap();
        let pending = guard.as_mut().ok_or("No sign-in is running.")?;
        (
            pending
                .receiver
                .take()
                .ok_or("The sign-in is already being awaited.")?,
            pending.verifier.clone(),
            pending.client_id.clone(),
        )
    };
    let answer = tokio::time::timeout(LOGIN_TIMEOUT, receiver.recv()).await;
    cancel_pending();
    let code = match answer {
        Ok(Some(Ok(code))) => code,
        Ok(Some(Err(refusal))) => return Err(format!("The authorization was refused: {refusal}")),
        Ok(None) => return Err("The sign-in was cancelled.".to_string()),
        Err(_) => {
            return Err(
                "The authorization timed out. Start it again when you are ready.".to_string(),
            );
        }
    };

    let response =
        client::exchange_code(&Endpoints::default(), &client_id, &code, &verifier).await?;
    if response.refresh_token.is_empty() {
        return Err(
            "The server sent no refresh token, so the login could not be kept.".to_string(),
        );
    }
    let refresh_token = response.refresh_token.clone();
    let probe = CommonsClient::new(
        &client_id,
        Tokens::from_response(response, "", client::now_secs(), ""),
        Arc::new(|_: &Tokens| {}),
    )?;
    let username = probe.whoami().await?;

    login_store::save(
        &session::login_file(&app_handle)?,
        &StoredLogin {
            refresh_token,
            username,
        },
    )?;
    session::reset();
    Ok(status(&app_handle))
}

#[tauri::command]
pub fn commons_logout(app_handle: AppHandle) -> Result<CommonsStatus, String> {
    cancel_pending();
    login_store::clear(&session::login_file(&app_handle)?);
    session::reset();
    Ok(status(&app_handle))
}

/// Ask Commons who the stored login belongs to.
#[tauri::command]
pub async fn commons_test_connection(app_handle: AppHandle) -> Result<String, String> {
    session::client(&app_handle)?.whoami().await
}

/// The folder to export into for an upload to Commons.
#[tauri::command]
pub fn commons_prepare_export(app_handle: AppHandle) -> Result<String, String> {
    export::prepare(&app_handle).map(|folder| folder.to_string_lossy().into_owned())
}

// ── The local catcher for the browser's redirect ─────────────────────────

async fn listen(
    listener: TcpListener,
    state: String,
    codes: mpsc::Sender<Result<String, String>>,
    mut stop: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut stop => return,
            accepted = listener.accept() => {
                let Ok((mut stream, _)) = accepted else { continue };
                if let Some(result) = handle_connection(&mut stream, &state).await {
                    let _ = codes.send(result).await;
                    return;
                }
            }
        }
    }
}

async fn answer(stream: &mut TcpStream, message: &str) {
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>RapidRAW</title></head>\
         <body><h2>RapidRAW</h2><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// `Some` when the connection carried the answer that ends the sign-in. Only
/// an answer with the right `state` is accepted, so a stray or forged redirect
/// cannot inject a foreign code.
async fn handle_connection(stream: &mut TcpStream, state: &str) -> Option<Result<String, String>> {
    let mut buffer = [0u8; 4096];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
        .await
        .ok()?
        .ok()?;
    let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
    let mut words = request.lines().next()?.split_whitespace();
    if words.next() != Some("GET") {
        return None;
    }
    let target = words.next()?;
    match oauth::parse_request_target(target) {
        Redirect::Code { code, state: got } if got == state => {
            answer(stream, "RapidRAW is authorized. You can close this window.").await;
            Some(Ok(code))
        }
        Redirect::Code { .. } => {
            answer(
                stream,
                "This answer does not belong to the running sign-in and was ignored.",
            )
            .await;
            None
        }
        Redirect::Failed(reason) => {
            answer(
                stream,
                "The authorization was refused. You can close this window.",
            )
            .await;
            Some(Err(reason))
        }
        Redirect::Other => {
            answer(stream, "Waiting for the authorization…").await;
            None
        }
    }
}
