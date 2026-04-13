use sqlx::PgPool;

use crate::clients::{solana::SolanaRpcClient, supabase::SupabaseAuthClient};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub solana: SolanaRpcClient,
    pub supabase_auth: SupabaseAuthClient,
    pub admin_emails: Vec<String>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        solana: SolanaRpcClient,
        supabase_auth: SupabaseAuthClient,
        admin_emails: Vec<String>,
    ) -> Self {
        Self {
            pool,
            solana,
            supabase_auth,
            admin_emails,
        }
    }
}
