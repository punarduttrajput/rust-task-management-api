use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{extract::FromRequestParts, http::request::Parts};
use chrono::Utc;
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{
    error::{AppError, AppResult},
    models::Role,
    AppState,
};

// ---------- passwords ----------

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

// ---------- one-time codes ----------

type HmacSha256 = Hmac<Sha256>;

pub fn generate_otp() -> String {
    format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
}

fn otp_mac(secret: &str, challenge_id: &str, code: &str) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(challenge_id.as_bytes());
    mac.update(b":");
    mac.update(code.as_bytes());
    mac
}

/// Keyed hash bound to the challenge, so the stored value is useless without the server secret.
pub fn hash_otp(secret: &str, challenge_id: &str, code: &str) -> String {
    hex::encode(otp_mac(secret, challenge_id, code).finalize().into_bytes())
}

/// Constant-time comparison against the stored hash.
pub fn verify_otp(secret: &str, challenge_id: &str, code: &str, stored_hex: &str) -> bool {
    let Ok(expected) = hex::decode(stored_hex) else { return false };
    otp_mac(secret, challenge_id, code).verify_slice(&expected).is_ok()
}

// ---------- JWT ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub role: Role,
    pub iat: i64,
    pub exp: i64,
}

pub fn issue_jwt(secret: &str, ttl_secs: i64, user_id: &str, email: &str, role: Role) -> AppResult<String> {
    let now = Utc::now().timestamp();
    let claims = Claims { sub: user_id.into(), email: email.into(), role, iat: now, exp: now + ttl_secs };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::Internal(e.to_string()))
}

pub fn decode_jwt(secret: &str, token: &str) -> Option<Claims> {
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default())
        .ok()
        .map(|d| d.claims)
}

/// Authenticated caller, extracted from `Authorization: Bearer <jwt>`.
#[derive(Debug, Clone)]
pub struct AuthUser(pub Claims);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| AppError::unauthorized("missing_token", "missing bearer token"))?;
        decode_jwt(&state.config.jwt_secret, token)
            .map(AuthUser)
            .ok_or_else(|| AppError::unauthorized("invalid_token", "invalid or expired token"))
    }
}

impl AuthUser {
    pub fn require_admin(&self) -> AppResult<()> {
        match self.0.role {
            Role::Admin => Ok(()),
            Role::Staff => Err(AppError::Forbidden("admin role required".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otp_hash_roundtrip() {
        let h = hash_otp("s", "c1", "123456");
        assert!(verify_otp("s", "c1", "123456", &h));
        assert!(!verify_otp("s", "c1", "654321", &h));
        assert!(!verify_otp("s", "c2", "123456", &h), "hash is bound to challenge");
        assert!(!h.contains("123456"));
    }

    #[test]
    fn jwt_roundtrip() {
        let t = issue_jwt("k", 60, "u1", "a@b.c", Role::Staff).unwrap();
        let c = decode_jwt("k", &t).unwrap();
        assert_eq!(c.sub, "u1");
        assert!(decode_jwt("other", &t).is_none());
    }

    #[test]
    fn password_roundtrip() {
        let h = hash_password("pw").unwrap();
        assert!(verify_password("pw", &h));
        assert!(!verify_password("nope", &h));
    }
}
