use rocket::serde::{Deserialize, Serialize, json::Json};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QuerySelect, Set};

use crate::db::postgres::PostgresPool;
use crate::entities::users;
use crate::guards::auth::Auth;
use crate::routes::pagination::Pagination;
use crate::utils::jwt::{TokenPair, generate_auth_tokens};
use crate::utils::response::ApiResponse;

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct CreateUserRequest {
    pub name: String,
    pub phone: String,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct LoginRequest {
    pub phone: String,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct DeleteUserResponse {
    pub id: i64,
}

#[rocket::post("/users", data = "<payload>")]
pub async fn create_user(
    payload: Json<CreateUserRequest>,
) -> ApiResponse<users::Model> {
    let name = payload.name.trim();
    let phone = payload.phone.trim();

    if let Some(validation_error) = validate_user_payload(name, phone) {
        return ApiResponse::bad_request(validation_error);
    }

    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    let user = users::ActiveModel {
        name: Set(name.to_string()),
        phone: Set(phone.to_string()),
        ..Default::default()
    };

    match user.insert(db).await {
        Ok(created_user) => ApiResponse::created(created_user, "User created successfully"),
        Err(error) => map_db_error(error, "Failed to create user"),
    }
}

#[rocket::get("/users?<pagination..>")]
pub async fn list_users(auth: Auth, pagination: Pagination) -> ApiResponse<Vec<users::Model>> {
    let _ = auth;

    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    match users::Entity::find()
        .limit(pagination.resolved_limit())
        .offset(pagination.resolved_skip())
        .all(db)
        .await
    {
        Ok(user_list) => ApiResponse::success(user_list, "Users fetched successfully"),
        Err(error) => ApiResponse::internal_error(format!("Failed to fetch users: {error}")),
    }
}

#[rocket::post("/login", data = "<payload>")]
pub async fn login(payload: Json<LoginRequest>) -> ApiResponse<TokenPair> {
    let phone = payload.phone.trim();
    if phone.is_empty() {
        return ApiResponse::bad_request("phone is required");
    }

    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    let user = match users::Entity::find()
        .filter(users::Column::Phone.eq(phone))
        .one(db)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => return ApiResponse::unauthorized("Invalid phone number"),
        Err(error) => return ApiResponse::internal_error(format!("Failed to login: {error}")),
    };

    match generate_auth_tokens(user.id, &user.phone) {
        Ok(tokens) => ApiResponse::success(tokens, "Login successful"),
        Err(error) => ApiResponse::internal_error(format!("Failed to login: {error}")),
    }
}

#[rocket::get("/users/<id>?<pagination..>")]
pub async fn get_user(id: i64, pagination: Pagination) -> ApiResponse<users::Model> {
    let _ = pagination;

    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    match users::Entity::find_by_id(id).one(db).await {
        Ok(Some(user)) => ApiResponse::success(user, "User fetched successfully"),
        Ok(None) => ApiResponse::not_found("User not found"),
        Err(error) => ApiResponse::internal_error(format!("Failed to fetch user: {error}")),
    }
}

#[rocket::put("/users/<id>", data = "<payload>")]
pub async fn update_user(id: i64, payload: Json<UpdateUserRequest>) -> ApiResponse<users::Model> {
    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    let Some(existing_user) = (match users::Entity::find_by_id(id).one(db).await {
        Ok(result) => result,
        Err(error) => return ApiResponse::internal_error(format!("Failed to fetch user: {error}")),
    }) else {
        return ApiResponse::not_found("User not found");
    };

    if payload.name.is_none() && payload.phone.is_none() {
        return ApiResponse::bad_request("Provide at least one field to update");
    }

    let mut user: users::ActiveModel = existing_user.into();

    if let Some(name) = &payload.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return ApiResponse::bad_request("name cannot be empty");
        }
        if trimmed.len() > 100 {
            return ApiResponse::bad_request("name must be at most 100 characters");
        }
        user.name = Set(trimmed.to_string());
    }

    if let Some(phone) = &payload.phone {
        let trimmed = phone.trim();
        if trimmed.is_empty() {
            return ApiResponse::bad_request("phone cannot be empty");
        }
        if trimmed.len() > 15 {
            return ApiResponse::bad_request("phone must be at most 15 characters");
        }
        user.phone = Set(trimmed.to_string());
    }

    match user.update(db).await {
        Ok(updated_user) => ApiResponse::success(updated_user, "User updated successfully"),
        Err(error) => map_db_error(error, "Failed to update user"),
    }
}

#[rocket::delete("/users/<id>")]
pub async fn delete_user(id: i64) -> ApiResponse<DeleteUserResponse> {
    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    let existing_user = match users::Entity::find()
        .filter(users::Column::Id.eq(id))
        .one(db)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => return ApiResponse::not_found("User not found"),
        Err(error) => return ApiResponse::internal_error(format!("Failed to fetch user: {error}")),
    };

    match users::Entity::delete_by_id(id).exec(db).await {
        Ok(_) => ApiResponse::success(
            DeleteUserResponse {
                id: existing_user.id,
            },
            "User deleted successfully",
        ),
        Err(error) => ApiResponse::internal_error(format!("Failed to delete user: {error}")),
    }
}

pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![
        create_user,
        list_users,
        login,
        get_user,
        update_user,
        delete_user
    ]
}

fn validate_user_payload(name: &str, phone: &str) -> Option<String> {
    if name.is_empty() {
        return Some("name is required".to_string());
    }
    if name.len() > 100 {
        return Some("name must be at most 100 characters".to_string());
    }
    if phone.is_empty() {
        return Some("phone is required".to_string());
    }
    if phone.len() > 15 {
        return Some("phone must be at most 15 characters".to_string());
    }

    None
}

fn map_db_error<T: Serialize>(error: sea_orm::DbErr, fallback_message: &str) -> ApiResponse<T> {
    let message = error.to_string();

    if message.contains("duplicate key value") || message.contains("users_phone_key") {
        return ApiResponse::bad_request("Phone already exists");
    }

    ApiResponse::internal_error(format!("{fallback_message}: {message}"))
}
