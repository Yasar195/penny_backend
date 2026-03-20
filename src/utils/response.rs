use rocket::http::{ContentType, Status};
use rocket::request::Request;
use rocket::response::{self, Responder, Response};
use rocket::serde::json::serde_json;
use rocket::serde::Serialize;
use std::io::Cursor;

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub status: u16,
    pub message: String,
    pub error: Option<String>,
    pub data: Option<T>,
}

impl<'r, T: Serialize> Responder<'r, 'static> for ApiResponse<T> {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let body = serde_json::to_string(&self).map_err(|_| Status::InternalServerError)?;
 
        Response::build()
            .status(Status::new(self.status))
            .header(ContentType::JSON)
            .sized_body(body.len(), Cursor::new(body))
            .ok()
    }
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            status: 200,
            message: message.into(),
            error: None,
            data: Some(data),
        }
    }
 
    pub fn created(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            status: 201,
            message: message.into(),
            error: None,
            data: Some(data),
        }
    }
 
    pub fn error(status: u16, message: impl Into<String>, error: impl Into<String>) -> ApiResponse<T> {
        ApiResponse {
            success: false,
            status,
            message: message.into(),
            error: Some(error.into()),
            data: None,
        }
    }
 
    pub fn not_found(message: impl Into<String>) -> ApiResponse<T> {
        Self::error(404, message, "Not Found")
    }
 
    pub fn unauthorized(message: impl Into<String>) -> ApiResponse<T> {
        Self::error(401, message, "Unauthorized")
    }
 
    pub fn bad_request(message: impl Into<String>) -> ApiResponse<T> {
        Self::error(400, message, "Bad Request")
    }
 
    pub fn internal_error(message: impl Into<String>) -> ApiResponse<T> {
        Self::error(500, message, "Internal Server Error")
    }
}