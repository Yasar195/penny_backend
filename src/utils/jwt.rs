use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rocket::serde::{Deserialize, Serialize};

const ACCESS_TOKEN_TTL_SECONDS: u64 = 15 * 60;
const REFRESH_TOKEN_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Claims {
    sub: String,
    token_type: String,
    user_id: i64,
    phone: String,
    iat: usize,
    exp: usize,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub phone: String,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub access_expires_in: u64,
    pub refresh_expires_in: u64,
}

pub fn generate_auth_tokens(user_id: i64, phone: &str) -> Result<TokenPair, String> {
    let access_secret = get_required_env("JWT_ACCESS_SECRET")?;
    let refresh_secret = get_required_env("JWT_REFRESH_SECRET")?;
    let now = current_unix_timestamp();

    let access_token = encode_token(
        &access_secret,
        Claims {
            sub: user_id.to_string(),
            token_type: "access".to_string(),
            user_id,
            phone: phone.to_string(),
            iat: now as usize,
            exp: (now + ACCESS_TOKEN_TTL_SECONDS) as usize,
        },
    )?;

    let refresh_token = encode_token(
        &refresh_secret,
        Claims {
            sub: user_id.to_string(),
            token_type: "refresh".to_string(),
            user_id,
            phone: phone.to_string(),
            iat: now as usize,
            exp: (now + REFRESH_TOKEN_TTL_SECONDS) as usize,
        },
    )?;

    Ok(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        access_expires_in: ACCESS_TOKEN_TTL_SECONDS,
        refresh_expires_in: REFRESH_TOKEN_TTL_SECONDS,
    })
}

pub fn verify_access_token(token: &str) -> Result<AuthUser, String> {
    let access_secret = get_required_env("JWT_ACCESS_SECRET")?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(access_secret.as_bytes()),
        &validation,
    )
    .map_err(|error| format!("Invalid access token: {error}"))?;

    if decoded.claims.token_type != "access" {
        return Err("Invalid token type".to_string());
    }

    Ok(AuthUser {
        user_id: decoded.claims.user_id,
        phone: decoded.claims.phone,
    })
}

fn encode_token(secret: &str, claims: Claims) -> Result<String, String> {
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|error| format!("Failed to generate token: {error}"))
}

fn get_required_env(name: &str) -> Result<String, String> {
    let value = env::var(name).map_err(|_| format!("{name} is not set"))?;
    if value.trim().is_empty() {
        return Err(format!("{name} is empty"));
    }
    Ok(value)
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_secs()
}
