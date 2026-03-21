use rocket::Route;

use crate::guards::auth::Auth;
use crate::routes::pagination::Pagination;
use crate::utils::response::ApiResponse;

pub mod pagination;
mod user;

#[rocket::get("/?<pagination..>")]
fn index(pagination: Pagination) -> ApiResponse<&'static str> {
    let _ = pagination;
    ApiResponse::success("Hello World", "Welcome to api")
}

#[rocket::get("/protected?<pagination..>")]
fn protected_route(auth: Auth, pagination: Pagination) -> ApiResponse<String> {
    let _ = pagination;
    ApiResponse::success(
        format!("this is protected data {}", auth.0),
        "Access granted".to_string(),
    )
}

pub fn routes() -> Vec<Route> {
    let mut app_routes = rocket::routes![index, protected_route];
    app_routes.extend(user::routes());
    app_routes
}
