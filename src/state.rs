use sqlx::PgPool;

use crate::{
    clients::{solana::SolanaRpcClient, supabase::SupabaseAuthClient},
    detector::DetectorRuntime,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub solana: SolanaRpcClient,
    pub supabase_auth: SupabaseAuthClient,
    pub admin_emails: Vec<String>,
    pub detector_runtime: DetectorRuntime,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        solana: SolanaRpcClient,
        supabase_auth: SupabaseAuthClient,
        admin_emails: Vec<String>,
        detector_runtime: DetectorRuntime,
    ) -> Self {
        Self {
            pool,
            solana,
            supabase_auth,
            admin_emails,
            detector_runtime,
        }
    }
}
