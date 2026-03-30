use sqlx::PgPool;

use crate::clients::{solana::SolanaRpcClient, supabase::SupabaseAuthClient};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub solana: SolanaRpcClient,
    pub supabase_auth: SupabaseAuthClient,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        solana: SolanaRpcClient,
        supabase_auth: SupabaseAuthClient,
    ) -> Self {
        Self {
            pool,
            solana,
            supabase_auth,
        }
    }
}
