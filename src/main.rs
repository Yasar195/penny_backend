use rocket::{get, launch, routes};

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocker() -> _ {
    rocket::build().mount("/", routes![index])
}