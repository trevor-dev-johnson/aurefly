use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

const SUPABASE_HTTP_TIMEOUT_SECS: u64 = 10;

#[derive(Clone)]
pub struct SupabaseAuthClient {
    http: Client,
    supabase_url: Option<String>,
    publishable_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SupabaseIdentity {
    pub supabase_user_id: Uuid,
    pub email: String,
    pub name: Option<String>,
}

impl SupabaseAuthClient {
    pub fn new(supabase_url: Option<String>, publishable_key: Option<String>) -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(SUPABASE_HTTP_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| Client::new()),
            supabase_url,
            publishable_key,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.supabase_url.is_some() && self.publishable_key.is_some()
    }

    pub fn redacted_supabase_url(&self) -> Option<String> {
        self.supabase_url.clone()
    }

    pub async fn get_user_for_token(&self, token: &str) -> AppResult<Option<SupabaseIdentity>> {
        let token = token.trim();
        if token.is_empty() {
            return Ok(None);
        }

        let supabase_url = self
            .supabase_url
            .as_deref()
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Supabase auth is not configured")))?;
        let publishable_key = self
            .publishable_key
            .as_deref()
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Supabase auth is not configured")))?;

        let endpoint = format!("{}/auth/v1/user", supabase_url.trim_end_matches('/'));
        let response = self
            .http
            .get(endpoint)
            .header("apikey", publishable_key)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| {
                AppError::Internal(anyhow::anyhow!(
                    "failed to contact Supabase Auth: {error}"
                ))
            })?;

        if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Internal(anyhow::anyhow!(
                "Supabase Auth failed with HTTP {status}: {body}"
            )));
        }

        let payload: SupabaseUserResponse = response
            .json()
            .await
            .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

        let supabase_user_id = Uuid::parse_str(payload.id.trim())
            .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;
        let email = payload
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Unauthorized("invalid Supabase user payload".to_string()))?
            .to_lowercase();
        let name = payload
            .user_metadata
            .and_then(|metadata| metadata.name.or(metadata.full_name))
            .and_then(clean_optional);

        Ok(Some(SupabaseIdentity {
            supabase_user_id,
            email,
            name,
        }))
    }
}

#[derive(Deserialize)]
struct SupabaseUserResponse {
    id: String,
    email: Option<String>,
    #[serde(default)]
    user_metadata: Option<SupabaseUserMetadata>,
}

#[derive(Default, Deserialize)]
struct SupabaseUserMetadata {
    name: Option<String>,
    full_name: Option<String>,
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
