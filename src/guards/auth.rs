use rocket;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};

use crate::utils::jwt::verify_access_token;

pub struct Auth {
    pub user_id: i64,
    pub phone: String,
}

#[derive(Debug)]
pub enum AuthError {
    Missing,
    Invalid,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Auth {
    type Error = AuthError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let Some(authorization) = request.headers().get_one("Authorization") else {
            return Outcome::Error((Status::Unauthorized, AuthError::Missing));
        };

        let Some(token) = authorization.strip_prefix("Bearer ") else {
            return Outcome::Error((Status::Unauthorized, AuthError::Invalid));
        };

        match verify_access_token(token.trim()) {
            Ok(user) => Outcome::Success(Auth {
                user_id: user.user_id,
                phone: user.phone,
            }),
            Err(_) => Outcome::Error((Status::Unauthorized, AuthError::Invalid)),
        }
    }
}
