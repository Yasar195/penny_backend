use rocket::{get, launch, routes};

use crate::utils::response::ApiResponse;
mod guards;
mod utils;

#[get("/")]
fn index() -> ApiResponse<&'static str> {
    ApiResponse::success("Hello World", "Welcome to api")
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![index])
}