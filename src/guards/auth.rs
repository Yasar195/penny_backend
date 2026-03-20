use rocket;
use rocket::http::Status;
use rocket::request::{ FromRequest, Outcome, Request };

pub struct Auth(pub String);

#[derive(Debug)]
pub enum AuthError {
    Missing,
    Invalid
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Auth {
    type Error = AuthError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("X-Api-Key") {
            Some(key) if is_valid(key) => Outcome::Success(Auth(key.to_string())),
            Some(_) => Outcome::Error((Status::Unauthorized, AuthError::Invalid)),
            None => Outcome::Error((Status::Unauthorized, AuthError::Missing))
        }
    }
}

fn is_valid(key: &str) -> bool {
    key == "secret_api_key"
}