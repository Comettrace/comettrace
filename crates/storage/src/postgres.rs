use sqlx::migrate::Migrator;

pub static CONTROL_PLANE: Migrator = sqlx::migrate!("migrations/postgres");

pub const SELF_HOSTED_PLAN: &str = "self-hosted";
