use std::collections::HashMap;
use std::env;
use std::fmt;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use rocket::serde::Deserialize;

const FIREBASE_CERTS_URL: &str =
    "https://www.googleapis.com/robot/v1/metadata/x509/securetoken@system.gserviceaccount.com";
const FIREBASE_ISSUER_BASE: &str = "https://securetoken.google.com";

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
struct FirebaseClaims {
    aud: String,
    iss: String,
    sub: String,
    exp: usize,
    iat: usize,
    phone_number: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FirebaseUser {
    pub phone: String,
    pub name: Option<String>,
}

#[derive(Debug)]
pub enum FirebaseAuthError {
    InvalidToken(String),
    Service(String),
}

impl fmt::Display for FirebaseAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FirebaseAuthError::InvalidToken(message) | FirebaseAuthError::Service(message) => {
                write!(f, "{message}")
            }
        }
    }
}

pub async fn verify_firebase_id_token(token: &str) -> Result<FirebaseUser, FirebaseAuthError> {
    let project_id = get_required_env("FIREBASE_PROJECT_ID")?;

    let header =
        decode_header(token).map_err(|error| FirebaseAuthError::InvalidToken(error.to_string()))?;
    if header.alg != Algorithm::RS256 {
        return Err(FirebaseAuthError::InvalidToken(
            "Invalid Firebase token algorithm".to_string(),
        ));
    }

    let kid = header.kid.ok_or_else(|| {
        FirebaseAuthError::InvalidToken("Firebase token is missing key id".to_string())
    })?;

    let cert = fetch_signing_cert(&kid).await?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    validation.set_audience(&[project_id.clone()]);
    validation.set_issuer(&[format!("{FIREBASE_ISSUER_BASE}/{project_id}")]);

    let decoded = decode::<FirebaseClaims>(
        token,
        &DecodingKey::from_rsa_pem(cert.as_bytes())
            .map_err(|error| FirebaseAuthError::Service(error.to_string()))?,
        &validation,
    )
    .map_err(|error| FirebaseAuthError::InvalidToken(error.to_string()))?;

    if decoded.claims.sub.trim().is_empty() {
        return Err(FirebaseAuthError::InvalidToken(
            "Firebase token subject is invalid".to_string(),
        ));
    }

    if decoded.claims.aud != project_id {
        return Err(FirebaseAuthError::InvalidToken(
            "Firebase token audience mismatch".to_string(),
        ));
    }

    let expected_issuer = format!("{FIREBASE_ISSUER_BASE}/{project_id}");
    if decoded.claims.iss != expected_issuer {
        return Err(FirebaseAuthError::InvalidToken(
            "Firebase token issuer mismatch".to_string(),
        ));
    }

    let phone = decoded
        .claims
        .phone_number
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            FirebaseAuthError::InvalidToken(
                "Firebase token does not contain a phone number".to_string(),
            )
        })?
        .to_string();

    let _ = decoded.claims.exp;
    let _ = decoded.claims.iat;

    Ok(FirebaseUser {
        phone,
        name: decoded.claims.name,
    })
}

async fn fetch_signing_cert(kid: &str) -> Result<String, FirebaseAuthError> {
    let certs = reqwest::get(FIREBASE_CERTS_URL)
        .await
        .map_err(|error| {
            FirebaseAuthError::Service(format!("Failed to fetch Firebase signing keys: {error}"))
        })?
        .json::<HashMap<String, String>>()
        .await
        .map_err(|error| {
            FirebaseAuthError::Service(format!("Failed to parse Firebase signing keys: {error}"))
        })?;

    certs.get(kid).cloned().ok_or_else(|| {
        FirebaseAuthError::InvalidToken("Firebase signing key was not found".to_string())
    })
}

fn get_required_env(name: &str) -> Result<String, FirebaseAuthError> {
    let value =
        env::var(name).map_err(|_| FirebaseAuthError::Service(format!("{name} is not set")))?;
    if value.trim().is_empty() {
        return Err(FirebaseAuthError::Service(format!("{name} is empty")));
    }
    Ok(value)
}
