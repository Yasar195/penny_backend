use rocket::serde::{Deserialize, Serialize, json::Json};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QuerySelect, Set};

use crate::db::postgres::PostgresPool;
use crate::entities::users;
use crate::guards::auth::Auth;
use crate::routes::pagination::Pagination;
use crate::utils::firebase::{FirebaseAuthError, verify_firebase_id_token};
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
    pub token: String,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct DeleteUserResponse {
    pub id: i64,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct UserResponse {
    pub id: i64,
    pub name: String,
}

#[rocket::post("/users", data = "<payload>")]
pub async fn create_user(payload: Json<CreateUserRequest>) -> ApiResponse<UserResponse> {
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
        Ok(created_user) => {
            ApiResponse::created(to_user_response(created_user), "User created successfully")
        }
        Err(error) => map_db_error(error, "Failed to create user"),
    }
}

#[rocket::get("/users?<pagination..>")]
pub async fn list_users(auth: Auth, pagination: Pagination) -> ApiResponse<Vec<UserResponse>> {
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
        Ok(user_list) => ApiResponse::success(
            user_list.into_iter().map(to_user_response).collect(),
            "Users fetched successfully",
        ),
        Err(error) => ApiResponse::internal_error(format!("Failed to fetch users: {error}")),
    }
}

#[rocket::post("/login", data = "<payload>")]
pub async fn login(payload: Json<LoginRequest>) -> ApiResponse<TokenPair> {
    let firebase_token = payload.token.trim();
    if firebase_token.is_empty() {
        return ApiResponse::bad_request("token is required");
    }

    let firebase_user = match verify_firebase_id_token(firebase_token).await {
        Ok(user) => user,
        Err(FirebaseAuthError::InvalidToken(error)) => {
            return ApiResponse::unauthorized(format!("Invalid Firebase token: {error}"));
        }
        Err(FirebaseAuthError::Service(error)) => {
            return ApiResponse::internal_error(format!("Firebase verification failed: {error}"));
        }
    };

    if firebase_user.phone.len() > 15 {
        return ApiResponse::bad_request("phone must be at most 15 characters");
    }

    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    let user = match users::Entity::find()
        .filter(users::Column::Phone.eq(firebase_user.phone.clone()))
        .one(db)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            let name = resolved_user_name(firebase_user.name.as_deref());
            let new_user = users::ActiveModel {
                name: Set(name),
                phone: Set(firebase_user.phone.clone()),
                ..Default::default()
            };

            match new_user.insert(db).await {
                Ok(created_user) => created_user,
                Err(error) if is_duplicate_phone_error(&error) => {
                    match users::Entity::find()
                        .filter(users::Column::Phone.eq(firebase_user.phone.clone()))
                        .one(db)
                        .await
                    {
                        Ok(Some(existing_user)) => existing_user,
                        Ok(None) => {
                            return ApiResponse::internal_error(
                                "Failed to resolve user after duplicate phone insert",
                            );
                        }
                        Err(fetch_error) => {
                            return ApiResponse::internal_error(format!(
                                "Failed to fetch user after duplicate phone insert: {fetch_error}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    return ApiResponse::internal_error(format!(
                        "Failed to create user during login: {error}"
                    ));
                }
            }
        }
        Err(error) => return ApiResponse::internal_error(format!("Failed to login: {error}")),
    };

    match generate_auth_tokens(user.id, &user.phone) {
        Ok(tokens) => ApiResponse::success(tokens, "Login successful"),
        Err(error) => ApiResponse::internal_error(format!("Failed to login: {error}")),
    }
}

#[rocket::get("/users/<id>?<pagination..>")]
pub async fn get_user(id: i64, pagination: Pagination) -> ApiResponse<UserResponse> {
    let _ = pagination;

    let db = match PostgresPool::connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return ApiResponse::internal_error(format!("Database connection error: {error}"));
        }
    };

    match users::Entity::find_by_id(id).one(db).await {
        Ok(Some(user)) => ApiResponse::success(to_user_response(user), "User fetched successfully"),
        Ok(None) => ApiResponse::not_found("User not found"),
        Err(error) => ApiResponse::internal_error(format!("Failed to fetch user: {error}")),
    }
}

#[rocket::put("/users/<id>", data = "<payload>")]
pub async fn update_user(id: i64, payload: Json<UpdateUserRequest>) -> ApiResponse<UserResponse> {
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
        Ok(updated_user) => {
            ApiResponse::success(to_user_response(updated_user), "User updated successfully")
        }
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

fn to_user_response(user: users::Model) -> UserResponse {
    UserResponse {
        id: user.id,
        name: user.name,
    }
}

fn resolved_user_name(firebase_name: Option<&str>) -> String {
    const MAX_USER_NAME_LENGTH: usize = 100;
    const DEFAULT_NAME: &str = "User";

    let Some(name) = firebase_name else {
        return DEFAULT_NAME.to_string();
    };

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return DEFAULT_NAME.to_string();
    }

    trimmed.chars().take(MAX_USER_NAME_LENGTH).collect()
}

fn is_duplicate_phone_error(error: &sea_orm::DbErr) -> bool {
    let message = error.to_string();
    message.contains("duplicate key value") || message.contains("users_phone_key")
}
