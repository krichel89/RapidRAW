//! OAuth 2.0 (PKCE) against Wikimedia: everything that needs no network.
//!
//! Mirrors Cammello's `mw_oauth2.py`. The client is registered as
//! NON-CONFIDENTIAL, so there is no client secret anywhere: the client id is
//! public by design and PKCE takes the secret's place.
//!
//! Facts this design rests on (taken over from Cammello, where they were
//! checked against the OAuth extension):
//!
//! * Access tokens expire after four hours; a refresh token renews them.
//! * The server ROTATES the refresh token on every renewal. Whoever refreshes
//!   must keep the new one; the old one is dead.
//! * Authorize and token endpoints live on meta, even for a consumer that is
//!   restricted to Commons. Authenticated API calls go to Commons.
//! * `redirect_uri` must be identical in both steps and match the registered
//!   URL exactly (OAuth 2 compares the full string, there is no prefix rule).

use base64::{Engine as _, engine::general_purpose};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const AUTHORIZE_URL: &str = "https://meta.wikimedia.org/w/rest.php/oauth2/authorize";
pub const TOKEN_URL: &str = "https://meta.wikimedia.org/w/rest.php/oauth2/access_token";

/// The callback registered with the OAuth consumer. Fixed, because the
/// registration cannot be edited afterwards and is compared exactly.
pub const LOOPBACK_PORT: u16 = 8128;
pub const REDIRECT_URI: &str = "http://127.0.0.1:8128/rapidraw/";

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

/// RFC 7636, method S256: base64url (no padding) of the SHA-256 of the verifier.
pub fn challenge_for(verifier: &str) -> String {
    general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A 128 character verifier (the maximum RFC 7636 allows) made of four random
/// UUIDs in hex, which stays inside the permitted alphabet `[A-Za-z0-9._~-]`.
pub fn new_pkce() -> Pkce {
    let verifier: String = (0..4)
        .map(|_| uuid::Uuid::new_v4().simple().to_string())
        .collect();
    let challenge = challenge_for(&verifier);
    Pkce {
        verifier,
        challenge,
    }
}

/// Unguessable `state`, tying the redirect to the login that started it.
pub fn new_state() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn authorize_url(client_id: &str, state: &str, challenge: &str) -> String {
    Url::parse_with_params(
        AUTHORIZE_URL,
        [
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", REDIRECT_URI),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ],
    )
    .map(|url| url.to_string())
    .unwrap_or_default()
}

/// The code out of text the user pasted: the bare code, or the whole address
/// from the browser's address bar after the redirect (the courtesy for a
/// firewall that blocked the loopback catch).
pub fn code_from_input(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.contains("://") || text.contains("code=") {
        let query = match text.split_once('?') {
            Some((_, query)) => query,
            None => text,
        };
        let query = query.split('#').next().unwrap_or("");
        return url_query_pairs(query)
            .into_iter()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.trim().to_string())
            .filter(|code| !code.is_empty());
    }
    Some(text.to_string())
}

/// The `state` of a pasted address, if it carries one.
pub fn state_from_input(text: &str) -> Option<String> {
    let query = text.trim().split_once('?')?.1;
    let query = query.split('#').next().unwrap_or("");
    url_query_pairs(query)
        .into_iter()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value)
}

fn url_query_pairs(query: &str) -> Vec<(String, String)> {
    Url::parse(&format!("http://localhost/?{query}"))
        .map(|url| {
            url.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, PartialEq)]
pub enum Redirect {
    /// The authorization answer: code and state.
    Code { code: String, state: String },
    /// The user (or the server) refused.
    Failed(String),
    /// Anything else (a favicon request, a probe).
    Other,
}

/// Read the request target of the browser's redirect (`/rapidraw/?code=…`).
pub fn parse_request_target(target: &str) -> Redirect {
    let Ok(base) = Url::parse("http://127.0.0.1") else {
        return Redirect::Other;
    };
    let Ok(url) = base.join(target) else {
        return Redirect::Other;
    };
    let mut code = String::new();
    let mut state = String::new();
    let mut error = String::new();
    let mut description = String::new();
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = value.into_owned(),
            "state" => state = value.into_owned(),
            "error" => error = value.into_owned(),
            "error_description" => description = value.into_owned(),
            _ => {}
        }
    }
    if !error.is_empty() {
        let text = if description.is_empty() {
            error
        } else {
            format!("{error}: {description}")
        };
        Redirect::Failed(text)
    } else if !code.is_empty() {
        Redirect::Code { code, state }
    } else {
        Redirect::Other
    }
}

/// What the token endpoint answers on success.
#[derive(Deserialize, Debug, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: i64,
}

/// The tokens of one signed-in user. Deliberately without `Debug` output of
/// the secrets.
#[derive(Clone, Default, PartialEq)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds; 0 when unknown.
    pub expires_at: i64,
    pub username: String,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("username", &self.username)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Tokens {
    /// The tokens after an exchange or a refresh. A response without a refresh
    /// token keeps the previous one rather than losing the login.
    pub fn from_response(
        response: TokenResponse,
        username: &str,
        now: i64,
        previous_refresh: &str,
    ) -> Self {
        let refresh_token = if response.refresh_token.is_empty() {
            previous_refresh.to_string()
        } else {
            response.refresh_token
        };
        Self {
            access_token: response.access_token,
            refresh_token,
            expires_at: if response.expires_in > 0 {
                now + response.expires_in
            } else {
                0
            },
            username: username.to_string(),
        }
    }

    /// Whether the access token is gone or about to be.
    pub fn needs_refresh(&self, now: i64, margin_secs: i64) -> bool {
        self.access_token.is_empty()
            || (self.expires_at > 0 && self.expires_at - now <= margin_secs)
    }
}

/// The part of a login that is kept between sessions. The access token is
/// not: it lives four hours and is cheap to get again, and it keeps the
/// credential store entry small (Windows limits an entry to about 2.5 KB).
#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct StoredLogin {
    pub refresh_token: String,
    pub username: String,
}

impl std::fmt::Debug for StoredLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredLogin")
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// The message of a failed token request: the server's own words if it sent
/// any (`message`, `error`, `hint`), else the HTTP status.
pub fn token_error_text(status: u16, body: &str) -> String {
    let json: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let field = |name: &str| json.get(name).and_then(|v| v.as_str()).map(str::to_string);
    let mut detail = field("message")
        .or_else(|| field("error"))
        .unwrap_or_else(|| format!("HTTP {status}"));
    if let Some(hint) = field("hint") {
        detail = format!("{detail} ({hint})");
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_the_rfc_7636_example() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_fits_the_rfc() {
        let pkce = new_pkce();
        assert_eq!(pkce.verifier.len(), 128);
        assert!(
            pkce.verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        );
        assert_eq!(pkce.challenge, challenge_for(&pkce.verifier));
        assert!(!pkce.challenge.contains('='), "no padding");
        assert_ne!(new_pkce().verifier, pkce.verifier);
        assert_ne!(new_state(), new_state());
    }

    #[test]
    fn authorize_url_carries_everything() {
        let url = Url::parse(&authorize_url("abc123", "st", "chal")).unwrap();
        assert_eq!(url.host_str(), Some("meta.wikimedia.org"));
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["response_type"], "code");
        assert_eq!(pairs["client_id"], "abc123");
        assert_eq!(pairs["redirect_uri"], REDIRECT_URI);
        assert_eq!(pairs["state"], "st");
        assert_eq!(pairs["code_challenge"], "chal");
        assert_eq!(pairs["code_challenge_method"], "S256");
    }

    #[test]
    fn pasted_code_forms() {
        assert_eq!(code_from_input("  abc  ").as_deref(), Some("abc"));
        assert_eq!(code_from_input(""), None);
        assert_eq!(
            code_from_input("http://127.0.0.1:8128/rapidraw/?code=xyz&state=s").as_deref(),
            Some("xyz")
        );
        assert_eq!(code_from_input("code=xyz&state=s").as_deref(), Some("xyz"));
        assert_eq!(
            code_from_input("http://127.0.0.1:8128/rapidraw/?state=s"),
            None
        );
        assert_eq!(
            state_from_input("http://127.0.0.1:8128/rapidraw/?code=xyz&state=s").as_deref(),
            Some("s")
        );
        assert_eq!(state_from_input("xyz"), None);
    }

    #[test]
    fn redirect_parsing() {
        assert_eq!(
            parse_request_target("/rapidraw/?code=c%2B1&state=s"),
            Redirect::Code {
                code: "c+1".into(),
                state: "s".into()
            }
        );
        assert_eq!(
            parse_request_target("/rapidraw/?error=access_denied&error_description=User+said+no"),
            Redirect::Failed("access_denied: User said no".into())
        );
        assert_eq!(parse_request_target("/favicon.ico"), Redirect::Other);
        assert_eq!(parse_request_target("//"), Redirect::Other);
    }

    #[test]
    fn refresh_response_without_a_new_refresh_token_keeps_the_old_one() {
        let response = TokenResponse {
            access_token: "a2".into(),
            refresh_token: String::new(),
            expires_in: 14400,
        };
        let tokens = Tokens::from_response(response, "Harald", 1000, "r1");
        assert_eq!(tokens.refresh_token, "r1");
        assert_eq!(tokens.expires_at, 15400);
        let rotated = TokenResponse {
            access_token: "a3".into(),
            refresh_token: "r2".into(),
            expires_in: 0,
        };
        let tokens = Tokens::from_response(rotated, "Harald", 1000, "r1");
        assert_eq!(tokens.refresh_token, "r2");
        assert_eq!(tokens.expires_at, 0);
    }

    #[test]
    fn refresh_margin() {
        let tokens = Tokens {
            access_token: "a".into(),
            expires_at: 1000,
            ..Default::default()
        };
        assert!(!tokens.needs_refresh(500, 120));
        assert!(tokens.needs_refresh(900, 120));
        assert!(Tokens::default().needs_refresh(0, 0), "no access token");
        let unknown = Tokens {
            access_token: "a".into(),
            ..Default::default()
        };
        assert!(!unknown.needs_refresh(1_000_000, 120), "unknown expiry");
    }

    #[test]
    fn secrets_stay_out_of_debug_output() {
        let tokens = Tokens {
            access_token: "SECRET-A".into(),
            refresh_token: "SECRET-R".into(),
            expires_at: 1,
            username: "Harald".into(),
        };
        let text = format!("{tokens:?}");
        assert!(!text.contains("SECRET") && text.contains("Harald"));
        let stored = StoredLogin {
            refresh_token: "SECRET-R".into(),
            username: "Harald".into(),
        };
        assert!(!format!("{stored:?}").contains("SECRET"));
    }

    #[test]
    fn token_errors_use_the_servers_words() {
        assert_eq!(
            token_error_text(
                400,
                r#"{"error":"invalid_grant","hint":"Token has expired"}"#
            ),
            "invalid_grant (Token has expired)"
        );
        assert_eq!(token_error_text(502, "<html>"), "HTTP 502");
        assert_eq!(
            token_error_text(400, r#"{"message":"The code is wrong"}"#),
            "The code is wrong"
        );
    }
}
