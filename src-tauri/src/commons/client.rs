//! The Commons side: sign in with OAuth 2.0, upload a file, write its
//! structured data.
//!
//! Mirrors Cammello's `api.py`. Every request carries the Bearer token; an
//! expired one is refreshed once and the request repeated, exactly as there.
//! Only reqwest features that RapidRAW already enables are used (no `query`,
//! no `form`): GET parameters go into the URL, POST bodies are multipart, and
//! the token endpoint gets a hand-made urlencoded body.

use reqwest::multipart::{Form, Part};
use reqwest::{Client, RequestBuilder, Url};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use super::oauth::{self, TokenResponse, Tokens};

pub const API_URL: &str = "https://commons.wikimedia.org/w/api.php";

/// Called with every new token set, so the rotated refresh token is kept.
pub type Persist = Arc<dyn Fn(&Tokens) + Send + Sync>;

#[derive(Clone)]
pub struct Endpoints {
    pub api: String,
    pub token: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            api: API_URL.to_string(),
            token: oauth::TOKEN_URL.to_string(),
        }
    }
}

pub struct CommonsClient {
    http: Client,
    client_id: String,
    endpoints: Endpoints,
    tokens: Mutex<Tokens>,
    csrf: Mutex<Option<String>>,
    persist: Persist,
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn truncated(text: &str) -> String {
    const LIMIT: usize = 600;
    if text.chars().count() <= LIMIT {
        text.to_string()
    } else {
        let head: String = text.chars().take(LIMIT).collect();
        format!("{head}… [{} characters]", text.chars().count())
    }
}

fn network_error(what: &str, error: &reqwest::Error) -> String {
    let mut text = format!("Network error during {what}: {error}");
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(&format!(" ({cause})"));
        source = cause.source();
    }
    text
}

/// A urlencoded request body without reqwest's `form` feature.
fn urlencoded(pairs: &[(&str, &str)]) -> String {
    let mut url = Url::parse("http://localhost/").expect("static url");
    url.query_pairs_mut().extend_pairs(pairs.iter().copied());
    url.query().unwrap_or("").to_string()
}

fn with_params(base: &str, pairs: &[(&str, &str)]) -> Result<Url, String> {
    Url::parse_with_params(base, pairs.iter().copied()).map_err(|e| e.to_string())
}

/// `(code, info)` of an API error object, if the answer is one.
pub fn api_error(json: &Value) -> Option<(String, String)> {
    let error = json.get("error")?;
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let info = error
        .get("info")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string());
    Some((code, info))
}

/// Whether an answer says the access token no longer works: HTTP 401 from the
/// REST layer, or an action-API answer with an error code
/// `mwoauth-invalid-authorization…` (how the OAuth extension reports it).
fn looks_expired(status: u16, json: &Value) -> bool {
    status == 401
        || api_error(json)
            .is_some_and(|(code, _)| code.starts_with("mwoauth-invalid-authorization"))
}

/// What the upload API said.
#[derive(Debug, PartialEq)]
pub enum UploadResult {
    Done,
}

/// Read the answer of `action=upload`. `Err((code, message))`; the code lets
/// the caller retry on `badtoken`.
pub fn parse_upload_response(
    json: &Value,
    filename: &str,
    ignore_warnings: bool,
) -> Result<UploadResult, (String, String)> {
    if let Some((code, info)) = api_error(json) {
        return Err((code.clone(), format!("[{code}] {info}")));
    }
    let upload = json.get("upload").unwrap_or(&Value::Null);
    let result = upload.get("result").and_then(Value::as_str);
    if result == Some("Success") {
        return Ok(UploadResult::Done);
    }
    if let Some(warnings) = upload.get("warnings").and_then(Value::as_object)
        && !warnings.is_empty()
    {
        if warnings.contains_key("exists") && ignore_warnings {
            return Ok(UploadResult::Done);
        }
        let message = if let Some(stored) = warnings.get("badfilename") {
            format!(
                "badfilename: MediaWiki would store \"{}\" instead of \"{filename}\". Change the target file name.",
                stored.as_str().unwrap_or("?").replace('_', " ")
            )
        } else if let Some(exists) = warnings.get("exists") {
            format!(
                "A file named \"{}\" already exists on Commons.",
                exists.as_str().unwrap_or(filename).replace('_', " ")
            )
        } else if let Some(duplicate) = warnings.get("duplicate") {
            format!("The same file is already on Commons: {duplicate}")
        } else {
            let detail: Vec<String> = warnings
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            format!("Warnings: {}", detail.join(", "))
        };
        return Err(("warnings".to_string(), message));
    }
    Err((
        "unexpected".to_string(),
        format!(
            "Upload failed (result={}). Response: {}",
            result.unwrap_or("none"),
            truncated(&json.to_string())
        ),
    ))
}

fn mime_for(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("tif" | "tiff") => "image/tiff",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// Exchange an authorization code for tokens. `client_id` and the redirect
/// URI must be the ones of the authorize step.
pub async fn exchange_code(
    endpoints: &Endpoints,
    client_id: &str,
    code: &str,
    verifier: &str,
) -> Result<TokenResponse, String> {
    token_request(
        &new_http()?,
        endpoints,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
            ("redirect_uri", oauth::REDIRECT_URI),
            ("code_verifier", verifier),
        ],
    )
    .await
}

fn new_http() -> Result<Client, String> {
    Client::builder()
        .user_agent(format!(
            "RapidRAW/{} (https://github.com/krichel89/RapidRAW; Commons upload)",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

async fn token_request(
    http: &Client,
    endpoints: &Endpoints,
    pairs: &[(&str, &str)],
) -> Result<TokenResponse, String> {
    let response = http
        .post(&endpoints.token)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(urlencoded(pairs))
        .send()
        .await
        .map_err(|e| network_error("the token exchange", &e))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    match serde_json::from_str::<TokenResponse>(&body) {
        Ok(tokens) if status == 200 && !tokens.access_token.is_empty() => Ok(tokens),
        _ => Err(format!(
            "The token exchange failed: {}",
            oauth::token_error_text(status, &body)
        )),
    }
}

impl CommonsClient {
    pub fn new(client_id: &str, tokens: Tokens, persist: Persist) -> Result<Self, String> {
        Self::with_endpoints(client_id, tokens, persist, Endpoints::default())
    }

    pub fn with_endpoints(
        client_id: &str,
        tokens: Tokens,
        persist: Persist,
        endpoints: Endpoints,
    ) -> Result<Self, String> {
        Ok(Self {
            http: new_http()?,
            client_id: client_id.to_string(),
            endpoints,
            tokens: Mutex::new(tokens),
            csrf: Mutex::new(None),
            persist,
        })
    }

    pub async fn username(&self) -> String {
        self.tokens.lock().await.username.clone()
    }

    /// Trade the refresh token for a new pair. `failed` is the access token
    /// that was rejected: when another task has already replaced it, nothing
    /// more is done (the refresh token is single-use).
    async fn refresh(&self, failed: &str) -> Result<(), String> {
        let mut tokens = self.tokens.lock().await;
        if tokens.access_token != failed && !tokens.access_token.is_empty() {
            return Ok(());
        }
        if tokens.refresh_token.is_empty() {
            return Err(
                "The Commons sign-in has expired. Sign in again in the export settings."
                    .to_string(),
            );
        }
        let response = token_request(
            &self.http,
            &self.endpoints,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &tokens.refresh_token),
                ("client_id", &self.client_id),
            ],
        )
        .await
        .map_err(|e| format!("{e}. Sign in again in the export settings."))?;
        // The server rotates the refresh token: the old one is dead now.
        let next = Tokens::from_response(
            response,
            &tokens.username,
            now_secs(),
            &tokens.refresh_token,
        );
        (self.persist)(&next);
        *tokens = next;
        self.csrf.lock().await.take();
        Ok(())
    }

    /// Refresh ahead of a long upload so it does not have to be sent twice.
    pub async fn ensure_fresh(&self) -> Result<(), String> {
        let (stale, token) = {
            let tokens = self.tokens.lock().await;
            (
                tokens.needs_refresh(now_secs(), 300),
                tokens.access_token.clone(),
            )
        };
        if stale {
            self.refresh(&token).await
        } else {
            Ok(())
        }
    }

    /// One authenticated request; refreshed and repeated once if the token
    /// turns out to be dead. Answers must be JSON with status 200.
    async fn send(
        &self,
        what: &str,
        build: impl Fn(&Client) -> Result<RequestBuilder, String>,
    ) -> Result<Value, String> {
        let mut refreshed = false;
        loop {
            let token = self.tokens.lock().await.access_token.clone();
            if token.is_empty() && !refreshed {
                // A stored login keeps only the refresh token.
                self.refresh("").await?;
                refreshed = true;
                continue;
            }
            let response = build(&self.http)?
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| network_error(what, &e))?;
            let status = response.status().as_u16();
            let text = response.text().await.map_err(|e| network_error(what, &e))?;
            let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if !refreshed && looks_expired(status, &json) {
                self.refresh(&token).await?;
                refreshed = true;
                continue;
            }
            if status != 200 {
                return Err(format!(
                    "HTTP {status} during {what}. Response: {}",
                    truncated(&text)
                ));
            }
            if json.is_null() {
                return Err(format!(
                    "Non-JSON answer during {what} (rate limit, maintenance or file too large?). Response: {}",
                    truncated(&text)
                ));
            }
            return Ok(json);
        }
    }

    fn get(&self, pairs: &[(&str, &str)]) -> Result<RequestBuilder, String> {
        Ok(self.http.get(with_params(&self.endpoints.api, pairs)?))
    }

    /// Who the token belongs to, asked of Commons itself (never of meta: a
    /// Commons-restricted token is refused there, Cammello 0.12.2).
    pub async fn whoami(&self) -> Result<String, String> {
        let json = self
            .send("the sign-in check", |_| {
                self.get(&[
                    ("action", "query"),
                    ("meta", "userinfo"),
                    ("format", "json"),
                ])
            })
            .await?;
        if let Some((code, info)) = api_error(&json) {
            return Err(format!("[{code}] {info}"));
        }
        let info = &json["query"]["userinfo"];
        match info["name"].as_str() {
            Some(name) if info.get("anon").is_none() => Ok(name.to_string()),
            _ => Err("The server did not recognise the sign-in.".to_string()),
        }
    }

    async fn csrf_token(&self) -> Result<String, String> {
        if let Some(token) = self.csrf.lock().await.clone() {
            return Ok(token);
        }
        let json = self
            .send("the edit token", |_| {
                self.get(&[("action", "query"), ("meta", "tokens"), ("format", "json")])
            })
            .await?;
        if let Some((code, info)) = api_error(&json) {
            return Err(format!("[{code}] {info}"));
        }
        let token = json["query"]["tokens"]["csrftoken"]
            .as_str()
            .ok_or_else(|| {
                format!(
                    "No edit token received. Response: {}",
                    truncated(&json.to_string())
                )
            })?
            .to_string();
        *self.csrf.lock().await = Some(token.clone());
        Ok(token)
    }

    async fn forget_csrf(&self) {
        self.csrf.lock().await.take();
    }

    /// POST an action as multipart; the token goes last among the text parts
    /// (MediaWiki wants it after any text that could contain `~~~~`).
    fn post(&self, fields: &[(&str, String)], token: &str, file: Option<Part>) -> RequestBuilder {
        let mut form = Form::new();
        for (name, value) in fields {
            form = form.text(name.to_string(), value.clone());
        }
        form = form
            .text("token", token.to_string())
            .text("format", "json")
            .text("assert", "user");
        if let Some(part) = file {
            form = form.part("file", part);
        }
        self.http.post(&self.endpoints.api).multipart(form)
    }

    pub async fn upload(
        &self,
        filename: &str,
        bytes: Vec<u8>,
        text: &str,
        comment: &str,
        ignore_warnings: bool,
    ) -> Result<(), String> {
        self.ensure_fresh().await?;
        for attempt in 1..=2 {
            let token = self.csrf_token().await?;
            let mut fields = vec![
                ("action", "upload".to_string()),
                ("filename", filename.to_string()),
                ("text", text.to_string()),
                ("comment", comment.to_string()),
            ];
            if ignore_warnings {
                fields.push(("ignorewarnings", "1".to_string()));
            }
            let json = self
                .send(&format!("upload of {filename}"), |_| {
                    let part = Part::bytes(bytes.clone())
                        .file_name(filename.to_string())
                        .mime_str(mime_for(filename))
                        .map_err(|e| e.to_string())?;
                    Ok(self
                        .post(&fields, &token, Some(part))
                        .timeout(Duration::from_secs(900)))
                })
                .await?;
            match parse_upload_response(&json, filename, ignore_warnings) {
                Ok(_) => return Ok(()),
                Err((code, _)) if code == "badtoken" && attempt == 1 => self.forget_csrf().await,
                Err((_, message)) => return Err(message),
            }
        }
        Err("Upload failed after retrying with a new edit token.".to_string())
    }

    /// The page id of `File:filename`, `None` if the page does not exist.
    pub async fn page_id(&self, filename: &str) -> Result<Option<u64>, String> {
        let title = format!("File:{filename}");
        let json = self
            .send("the page id", |_| {
                self.get(&[("action", "query"), ("titles", &title), ("format", "json")])
            })
            .await?;
        if let Some((code, info)) = api_error(&json) {
            return Err(format!("[{code}] {info}"));
        }
        Ok(json["query"]["pages"]
            .as_object()
            .and_then(|pages| pages.values().next())
            .and_then(|page| page["pageid"].as_u64()))
    }

    /// Labels and statements of the file (`wbeditentity` on `M<pageid>`).
    pub async fn set_structured_data(&self, page_id: u64, data: &Value) -> Result<(), String> {
        let entity = format!("M{page_id}");
        for attempt in 1..=2 {
            let token = self.csrf_token().await?;
            let fields = vec![
                ("action", "wbeditentity".to_string()),
                ("id", entity.clone()),
                ("data", data.to_string()),
            ];
            let json = self
                .send(&format!("structured data of {entity}"), |_| {
                    Ok(self.post(&fields, &token, None))
                })
                .await?;
            match api_error(&json) {
                None => return Ok(()),
                Some((code, _))
                    if (code == "badtoken" || code == "invalid-csrf-token") && attempt == 1 =>
                {
                    self.forget_csrf().await;
                }
                Some((code, info)) => return Err(format!("[{code}] {info}")),
            }
        }
        Err("wbeditentity failed after retrying with a new edit token.".to_string())
    }

    /// Purge with a links update, so that a file with new "depicts" leaves the
    /// "missing structured data" maintenance categories at once. Not essential.
    pub async fn purge(&self, filename: &str) -> Result<(), String> {
        let token = self.csrf_token().await?;
        let fields = vec![
            ("action", "purge".to_string()),
            ("titles", format!("File:{filename}")),
            ("forcelinkupdate", "1".to_string()),
        ];
        let json = self
            .send("the purge", |_| Ok(self.post(&fields, &token, None)))
            .await?;
        match api_error(&json) {
            None => Ok(()),
            Some((code, info)) => Err(format!("[{code}] {info}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn upload_answers() {
        let ok = json!({ "upload": { "result": "Success" } });
        assert_eq!(
            parse_upload_response(&ok, "a.jpg", false),
            Ok(UploadResult::Done)
        );

        let token = json!({ "error": { "code": "badtoken", "info": "Invalid CSRF token." } });
        let (code, message) = parse_upload_response(&token, "a.jpg", false).unwrap_err();
        assert_eq!(code, "badtoken");
        assert!(message.contains("Invalid CSRF token"));

        let exists =
            json!({ "upload": { "result": "Warning", "warnings": { "exists": "A.jpg" } } });
        assert!(
            parse_upload_response(&exists, "A.jpg", false)
                .unwrap_err()
                .1
                .contains("already exists")
        );
        assert_eq!(
            parse_upload_response(&exists, "A.jpg", true),
            Ok(UploadResult::Done)
        );

        let bad =
            json!({ "upload": { "result": "Warning", "warnings": { "badfilename": "A_b.jpg" } } });
        let message = parse_upload_response(&bad, "A:b.jpg", false).unwrap_err().1;
        assert!(message.contains("\"A b.jpg\"") && message.contains("\"A:b.jpg\""));

        let other = json!({ "upload": { "result": "Failure" } });
        assert_eq!(
            parse_upload_response(&other, "a.jpg", false).unwrap_err().0,
            "unexpected"
        );
        assert_eq!(
            parse_upload_response(&Value::Null, "a.jpg", false)
                .unwrap_err()
                .0,
            "unexpected"
        );
    }

    #[test]
    fn dead_token_detection() {
        assert!(looks_expired(401, &Value::Null));
        assert!(looks_expired(
            200,
            &json!({ "error": { "code": "mwoauth-invalid-authorization-invalid-user", "info": "x" } })
        ));
        assert!(!looks_expired(
            200,
            &json!({ "error": { "code": "badtoken", "info": "x" } })
        ));
        assert!(!looks_expired(500, &Value::Null));
    }

    #[test]
    fn urlencoded_bodies() {
        assert_eq!(
            urlencoded(&[("a", "b c"), ("code", "x+y&z")]),
            "a=b+c&code=x%2By%26z"
        );
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for("a.JPG"), "image/jpeg");
        assert_eq!(mime_for("a.tiff"), "image/tiff");
        assert_eq!(mime_for("a.heic"), "application/octet-stream");
    }
}

/// The whole conversation with Commons against a local stand-in server, since
/// the real one cannot be called from a test.
#[cfg(test)]
mod conversation_tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex as StdMutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    #[derive(Default)]
    struct Seen {
        /// Answer the next upload with 401, as if the token died in between.
        reject_next_upload: bool,
        /// `path action` of every request, in order.
        calls: Vec<String>,
        uploads: Vec<String>,
        structured: Vec<String>,
    }

    async fn read_request(stream: &mut TcpStream) -> (String, String) {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        let head_end = loop {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "connection closed early");
            buffer.extend_from_slice(&chunk[..n]);
            if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
        let length = head
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while buffer.len() < head_end + length {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "connection closed early");
            buffer.extend_from_slice(&chunk[..n]);
        }
        let body = String::from_utf8_lossy(&buffer[head_end..head_end + length]).to_string();
        (head, body)
    }

    fn multipart_value(body: &str, name: &str) -> Option<String> {
        let marker = format!("name=\"{name}\"\r\n\r\n");
        let start = body.find(&marker)? + marker.len();
        Some(body[start..].split("\r\n--").next()?.to_string())
    }

    /// Answers like Commons for a client whose valid access token is `a2`
    /// (obtained by refreshing `r1`, which returns the rotated `r2`).
    async fn serve(listener: TcpListener, seen: Arc<StdMutex<Seen>>) {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let seen = seen.clone();
            tokio::spawn(async move {
                let (head, body) = read_request(&mut stream).await;
                let target = head.split_whitespace().nth(1).unwrap_or("").to_string();
                let authorized = head.contains("authorization: Bearer a2")
                    || head.contains("Authorization: Bearer a2");
                let (status, answer) = if target.starts_with("/token") {
                    seen.lock().unwrap().calls.push("token".into());
                    if body.contains("grant_type=refresh_token")
                        && (body.contains("refresh_token=r1") || body.contains("refresh_token=r2"))
                    {
                        (
                            200,
                            json!({ "access_token": "a2", "refresh_token": "r2", "expires_in": 14400 }),
                        )
                    } else {
                        (
                            400,
                            json!({ "error": "invalid_grant", "hint": "Token has expired" }),
                        )
                    }
                } else if !authorized {
                    seen.lock().unwrap().calls.push("rejected".into());
                    (401, json!({}))
                } else if target.contains("meta=userinfo") {
                    seen.lock().unwrap().calls.push("userinfo".into());
                    (
                        200,
                        json!({ "query": { "userinfo": { "id": 5, "name": "Harald Krichel" } } }),
                    )
                } else if target.contains("meta=tokens") {
                    seen.lock().unwrap().calls.push("csrf".into());
                    (
                        200,
                        json!({ "query": { "tokens": { "csrftoken": "CSRF+\\" } } }),
                    )
                } else if target.contains("titles=File%3AAnna+Meier.jpg") {
                    seen.lock().unwrap().calls.push("pageid".into());
                    (
                        200,
                        json!({ "query": { "pages": { "77": { "pageid": 77, "title": "File:Anna Meier.jpg" } } } }),
                    )
                } else {
                    let action = multipart_value(&body, "action").unwrap_or_default();
                    let token_ok = multipart_value(&body, "token").as_deref() == Some("CSRF+\\");
                    seen.lock().unwrap().calls.push(action.clone());
                    let reject = action == "upload" && {
                        let mut seen = seen.lock().unwrap();
                        std::mem::take(&mut seen.reject_next_upload)
                    };
                    if reject {
                        seen.lock().unwrap().calls.push("upload-rejected".into());
                        seen.lock().unwrap().calls.retain(|c| c != "upload");
                        (401, json!({}))
                    } else if !token_ok {
                        (
                            200,
                            json!({ "error": { "code": "badtoken", "info": "Invalid CSRF token." } }),
                        )
                    } else if action == "upload" {
                        seen.lock().unwrap().uploads.push(body.clone());
                        (200, json!({ "upload": { "result": "Success" } }))
                    } else if action == "wbeditentity" {
                        let mut seen = seen.lock().unwrap();
                        seen.structured.push(format!(
                            "{} {}",
                            multipart_value(&body, "id").unwrap_or_default(),
                            multipart_value(&body, "data").unwrap_or_default()
                        ));
                        (200, json!({ "success": 1 }))
                    } else {
                        (200, json!({ "purge": [] }))
                    }
                };
                let payload = answer.to_string();
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    }

    async fn client_with_expired_token() -> (
        CommonsClient,
        Arc<StdMutex<Seen>>,
        Arc<StdMutex<Vec<Tokens>>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(StdMutex::new(Seen::default()));
        tokio::spawn(serve(listener, seen.clone()));
        let persisted = Arc::new(StdMutex::new(Vec::new()));
        let sink = persisted.clone();
        let client = CommonsClient::with_endpoints(
            "client-id",
            Tokens {
                access_token: "a1".into(),
                refresh_token: "r1".into(),
                expires_at: 0,
                username: "Harald Krichel".into(),
            },
            Arc::new(move |tokens: &Tokens| sink.lock().unwrap().push(tokens.clone())),
            Endpoints {
                api: format!("http://127.0.0.1:{port}/api"),
                token: format!("http://127.0.0.1:{port}/token"),
            },
        )
        .unwrap();
        (client, seen, persisted)
    }

    #[tokio::test]
    async fn an_expired_token_is_refreshed_once_and_the_rotated_one_is_kept() {
        let (client, seen, persisted) = client_with_expired_token().await;
        assert_eq!(client.whoami().await.unwrap(), "Harald Krichel");
        assert_eq!(client.whoami().await.unwrap(), "Harald Krichel");
        assert_eq!(
            seen.lock().unwrap().calls,
            ["rejected", "token", "userinfo", "userinfo"],
            "one refresh, no loop"
        );
        let persisted = persisted.lock().unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(
            persisted[0].refresh_token, "r2",
            "the rotated refresh token"
        );
        assert_eq!(persisted[0].access_token, "a2");
        assert!(persisted[0].expires_at > 0);
    }

    #[tokio::test]
    async fn a_dead_refresh_token_says_so() {
        let (client, _seen, _persisted) = client_with_expired_token().await;
        *client.tokens.lock().await = Tokens {
            access_token: "a1".into(),
            refresh_token: "wrong".into(),
            ..Default::default()
        };
        let error = client.whoami().await.unwrap_err();
        assert!(
            error.contains("invalid_grant") && error.contains("Sign in again"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn the_file_is_sent_again_when_the_token_dies_during_the_upload() {
        let (client, seen, persisted) = client_with_expired_token().await;
        // Valid token, so the edit token is fetched fine; then the upload is refused once.
        client.tokens.lock().await.access_token = "a2".into();
        seen.lock().unwrap().reject_next_upload = true;
        client
            .upload(
                "Anna Meier.jpg",
                b"JPEGDATA".to_vec(),
                "text",
                "summary",
                false,
            )
            .await
            .unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.calls, ["csrf", "upload-rejected", "token", "upload"]);
        assert_eq!(seen.uploads.len(), 1);
        assert!(
            seen.uploads[0].contains("JPEGDATA"),
            "the retry carries the file, not an empty body"
        );
        assert_eq!(persisted.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn upload_then_structured_data() {
        let (client, seen, _persisted) = client_with_expired_token().await;
        // A known, passed expiry: the upload refreshes ahead of time.
        client.tokens.lock().await.expires_at = 1;
        let bytes = b"JPEGDATA\x00\xff".to_vec();
        client
            .upload(
                "Anna Meier.jpg",
                bytes,
                "== {{int:filedesc}} ==\nText ~~~~",
                "Uploaded with RapidRAW",
                false,
            )
            .await
            .unwrap();
        let page = client.page_id("Anna Meier.jpg").await.unwrap();
        assert_eq!(page, Some(77));
        let data = json!({ "labels": { "en": { "language": "en", "value": "Anna" } } });
        client.set_structured_data(77, &data).await.unwrap();
        client.purge("Anna Meier.jpg").await.unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.calls,
            ["token", "csrf", "upload", "pageid", "wbeditentity", "purge"],
            "ensure_fresh refreshes before the upload"
        );
        let upload = &seen.uploads[0];
        assert_eq!(
            multipart_value(upload, "filename").as_deref(),
            Some("Anna Meier.jpg")
        );
        assert_eq!(
            multipart_value(upload, "comment").as_deref(),
            Some("Uploaded with RapidRAW")
        );
        assert!(
            multipart_value(upload, "text")
                .unwrap()
                .ends_with("Text ~~~~")
        );
        assert_eq!(multipart_value(upload, "assert").as_deref(), Some("user"));
        assert!(upload.contains("filename=\"Anna Meier.jpg\""));
        assert!(upload.contains("Content-Type: image/jpeg"));
        let token_at = upload.find("name=\"token\"").unwrap();
        let file_at = upload.find("name=\"file\"").unwrap();
        assert!(token_at < file_at, "the file part comes last");
        assert!(seen.structured[0].starts_with("M77 {"));
        assert!(seen.structured[0].contains("\"value\":\"Anna\""));
    }
}
