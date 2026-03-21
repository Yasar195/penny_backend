use std::env;

use rocket::fairing::AdHoc;
use rocket::tokio::sync::OnceCell;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

pub struct PostgresPool;

static CONNECTION: OnceCell<DatabaseConnection> = OnceCell::const_new();

impl PostgresPool {
    pub async fn init() -> Result<&'static DatabaseConnection, DbErr> {
        CONNECTION
            .get_or_try_init(|| async {
                let options = build_connect_options()?;
                Database::connect(options).await
            })
            .await
    }

    pub async fn connection() -> Result<&'static DatabaseConnection, DbErr> {
        Self::init().await
    }
}

pub fn init_fairing() -> AdHoc {
    AdHoc::try_on_ignite("SeaORM Database Init", |rocket| async {
        match PostgresPool::init().await {
            Ok(_) => Ok(rocket),
            Err(error) => {
                eprintln!("Failed to initialize database connection: {error}");
                Err(rocket)
            }
        }
    })
}

fn build_connect_options() -> Result<ConnectOptions, DbErr> {
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| DbErr::Custom("DATABASE_URL is not set".to_string()))?;

    let max_connections = env::var("DATABASE_POOL_MAX_SIZE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(16);

    let min_connections = env::var("DATABASE_POOL_MIN_SIZE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);

    let mut options = ConnectOptions::new(database_url);
    options
        .max_connections(max_connections)
        .min_connections(min_connections)
        .sqlx_logging(false);

    Ok(options)
}
