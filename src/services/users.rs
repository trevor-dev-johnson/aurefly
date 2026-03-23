use sqlx::PgPool;

use crate::{
    error::{AppError, AppResult},
    models::user::User,
};

pub struct CreateUser {
    pub email: String,
    pub name: Option<String>,
}

pub async fn create(pool: &PgPool, input: CreateUser) -> AppResult<User> {
    let email = input.email.trim().to_lowercase();
    if email.is_empty() {
        return Err(AppError::Validation("email is required".to_string()));
    }

    let name = input.name.and_then(clean_optional);

    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (email, name)
        VALUES ($1, $2)
        RETURNING id, email, name, created_at
        "#,
    )
    .bind(email)
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok(user)
}

pub async fn list(pool: &PgPool) -> AppResult<Vec<User>> {
    let users = sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, name, created_at
        FROM users
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(users)
}

pub async fn get(pool: &PgPool, user_id: uuid::Uuid) -> AppResult<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, name, created_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(user)
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
