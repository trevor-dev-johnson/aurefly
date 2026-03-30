use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    clients::supabase::{SupabaseAuthClient, SupabaseIdentity},
    error::AppResult,
    models::user::User,
};

pub async fn user_for_token(
    pool: &PgPool,
    supabase_auth: &SupabaseAuthClient,
    token: &str,
) -> AppResult<Option<User>> {
    let Some(identity) = supabase_auth.get_user_for_token(token).await? else {
        return Ok(None);
    };

    let user = sync_user_from_identity(pool, &identity).await?;
    Ok(Some(user))
}

async fn sync_user_from_identity(pool: &PgPool, identity: &SupabaseIdentity) -> AppResult<User> {
    if let Some(user) = find_by_supabase_user_id(pool, identity.supabase_user_id).await? {
        return update_existing_user(
            pool,
            user.id,
            identity.supabase_user_id,
            &identity.email,
            identity.name.as_deref(),
        )
        .await;
    }

    if let Some(user) = find_by_email(pool, &identity.email).await? {
        return update_existing_user(
            pool,
            user.id,
            identity.supabase_user_id,
            &identity.email,
            identity.name.as_deref(),
        )
        .await;
    }

    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, supabase_user_id, email, name)
        VALUES ($1, $1, $2, $3)
        RETURNING id, email, name, created_at
        "#,
    )
    .bind(identity.supabase_user_id)
    .bind(&identity.email)
    .bind(&identity.name)
    .fetch_one(pool)
    .await?;

    Ok(user)
}

async fn find_by_supabase_user_id(pool: &PgPool, supabase_user_id: Uuid) -> AppResult<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, name, created_at
        FROM users
        WHERE supabase_user_id = $1
        "#,
    )
    .bind(supabase_user_id)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

async fn find_by_email(pool: &PgPool, email: &str) -> AppResult<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, name, created_at
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

async fn update_existing_user(
    pool: &PgPool,
    user_id: Uuid,
    supabase_user_id: Uuid,
    email: &str,
    name: Option<&str>,
) -> AppResult<User> {
    let user = sqlx::query_as::<_, User>(
        r#"
        UPDATE users
        SET
            supabase_user_id = COALESCE(supabase_user_id, $2),
            email = $3,
            name = COALESCE($4, name)
        WHERE id = $1
        RETURNING id, email, name, created_at
        "#,
    )
    .bind(user_id)
    .bind(supabase_user_id)
    .bind(email)
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok(user)
}
