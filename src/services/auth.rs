use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::user::User,
};

const SESSION_TTL_SQL: &str = "NOW() + INTERVAL '24 hours'";

pub struct RegisterUser {
    pub email: String,
    pub name: Option<String>,
    pub password: String,
}

pub struct SignInUser {
    pub email: String,
    pub password: String,
}

pub struct AuthSession {
    pub token: String,
    pub user: User,
}

pub async fn register(pool: &PgPool, input: RegisterUser) -> AppResult<AuthSession> {
    let email = normalize_email(&input.email)?;
    let name = input.name.and_then(clean_optional);
    validate_password(&input.password)?;

    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (email, name, password_hash)
        VALUES ($1, $2, crypt($3, gen_salt('bf', 12)))
        RETURNING id, email, name, created_at
        "#,
    )
    .bind(email)
    .bind(name)
    .bind(input.password)
    .fetch_one(pool)
    .await?;

    issue_session(pool, user).await
}

pub async fn sign_in(pool: &PgPool, input: SignInUser) -> AppResult<AuthSession> {
    let email = normalize_email(&input.email)?;
    validate_password(&input.password)?;

    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, name, created_at
        FROM users
        WHERE email = $1
          AND password_hash IS NOT NULL
          AND password_hash = crypt($2, password_hash)
        "#,
    )
    .bind(email)
    .bind(input.password)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid email or password".to_string()))?;

    issue_session(pool, user).await
}

pub async fn user_for_token(pool: &PgPool, token: &str) -> AppResult<Option<User>> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(None);
    }

    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT users.id, users.email, users.name, users.created_at
        FROM auth_sessions
        INNER JOIN users ON users.id = auth_sessions.user_id
        WHERE auth_sessions.token_hash = encode(digest($1, 'sha256'), 'hex')
          AND auth_sessions.expires_at > NOW()
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn revoke_token(pool: &PgPool, token: &str) -> AppResult<bool> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(false);
    }

    let deleted = sqlx::query(
        r#"
        DELETE FROM auth_sessions
        WHERE token_hash = encode(digest($1, 'sha256'), 'hex')
        "#,
    )
    .bind(token)
    .execute(pool)
    .await?;

    Ok(deleted.rows_affected() > 0)
}

async fn issue_session(pool: &PgPool, user: User) -> AppResult<AuthSession> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    sqlx::query(&format!(
        r#"
        INSERT INTO auth_sessions (user_id, token_hash, expires_at)
        VALUES ($1, encode(digest($2, 'sha256'), 'hex'), {SESSION_TTL_SQL})
        "#
    ))
    .bind(user.id)
    .bind(&token)
    .execute(pool)
    .await?;

    Ok(AuthSession { token, user })
}

fn normalize_email(value: &str) -> AppResult<String> {
    let email = value.trim().to_lowercase();
    if email.is_empty() {
        return Err(AppError::Validation("email is required".to_string()));
    }

    Ok(email)
}

fn validate_password(value: &str) -> AppResult<()> {
    if value.trim().len() < 8 {
        return Err(AppError::Validation(
            "password must be at least 8 characters".to_string(),
        ));
    }

    Ok(())
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
