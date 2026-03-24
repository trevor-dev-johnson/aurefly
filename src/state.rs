use sqlx::PgPool;

use crate::{clients::solana::SolanaRpcClient, rate_limit::AuthRateLimiter, treasury::TreasuryWallet};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub solana: SolanaRpcClient,
    pub treasury: TreasuryWallet,
    pub auth_rate_limiter: AuthRateLimiter,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        solana: SolanaRpcClient,
        treasury: TreasuryWallet,
        auth_rate_limiter: AuthRateLimiter,
    ) -> Self {
        Self {
            pool,
            solana,
            treasury,
            auth_rate_limiter,
        }
    }
}
