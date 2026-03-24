use sqlx::PgPool;

use crate::{clients::solana::SolanaRpcClient, rate_limit::AuthRateLimiter};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub solana: SolanaRpcClient,
    pub auth_rate_limiter: AuthRateLimiter,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        solana: SolanaRpcClient,
        auth_rate_limiter: AuthRateLimiter,
    ) -> Self {
        Self {
            pool,
            solana,
            auth_rate_limiter,
        }
    }
}
