use sqlx::PgPool;

use crate::{clients::solana::SolanaRpcClient, treasury::TreasuryWallet};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub solana: SolanaRpcClient,
    pub treasury: TreasuryWallet,
}

impl AppState {
    pub fn new(pool: PgPool, solana: SolanaRpcClient, treasury: TreasuryWallet) -> Self {
        Self {
            pool,
            solana,
            treasury,
        }
    }
}
