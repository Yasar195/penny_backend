use rocket::launch;

mod catchers;
mod db;
mod entities;
mod guards;
mod routes;
mod utils;

#[launch]
fn rocket() -> _ {
    dotenvy::dotenv().ok();

    rocket::build()
        .attach(db::postgres::init_fairing())
        .mount("/", routes::routes())
        .register("/", catchers::catchers())
}
