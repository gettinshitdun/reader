use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};

use super::auth::{self, User};

pub const SESSION_COOKIE_NAME: &str = "reader_session";

/// Extractor that resolves the authenticated user from the session cookie.
/// Returns 401 if no valid session exists (used for API routes).
pub struct AuthUser(pub User);

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = extract_session_token(parts).ok_or(StatusCode::UNAUTHORIZED)?;
        let user = auth::validate_session(&token)
            .await
            .ok_or(StatusCode::UNAUTHORIZED)?;
        Ok(AuthUser(user))
    }
}

/// Extracts the session token from the Cookie header.
pub fn extract_session_token(parts: &Parts) -> Option<String> {
    let cookie_header = parts.headers.get("cookie")?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(SESSION_COOKIE_NAME) {
            let value = value.strip_prefix('=')?;
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Non-failing extractor: returns Option<User>. Used by page routes that
/// redirect to /login instead of returning 401.
pub struct MaybeUser(pub Option<User>);

impl<S: Send + Sync> FromRequestParts<S> for MaybeUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user = async {
            let token = extract_session_token(parts)?;
            auth::validate_session(&token).await
        }
        .await;
        Ok(MaybeUser(user))
    }
}
