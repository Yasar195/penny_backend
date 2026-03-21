use rocket::Catcher;

use crate::utils::response::ApiResponse;

#[rocket::catch(401)]
fn unauthorized() -> ApiResponse<()> {
    ApiResponse::unauthorized("The request requires user authentication.")
}

#[rocket::catch(422)]
fn malformed_request() -> ApiResponse<()> {
    ApiResponse::malformed_error("The request is malformed.")
}

#[rocket::catch(404)]
fn not_found() -> ApiResponse<()> {
    ApiResponse::not_found("The requested resource not found.")
}

pub fn catchers() -> Vec<Catcher> {
    rocket::catchers![unauthorized, malformed_request, not_found]
}
