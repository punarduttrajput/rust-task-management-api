pub mod auth;
pub mod cache;
pub mod config;
pub mod email;
pub mod error;
pub mod models;
pub mod routes;

use std::{str::FromStr, sync::Arc, time::Duration};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

use crate::{cache::TtlCache, config::Config, email::DevMailer, models::MyTasks};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub mailer: Arc<DevMailer>,
    pub my_tasks_cache: Arc<TtlCache<MyTasks>>,
}

pub async fn build_state(config: Config) -> Result<AppState, BoxError> {
    let opts = SqliteConnectOptions::from_str(&config.database_url)?
        .create_if_missing(true)
        .foreign_keys(true);
    // An in-memory SQLite DB is per-connection, so tests need a single connection.
    let max = if config.database_url.contains(":memory:") { 1 } else { 5 };
    let db = SqlitePoolOptions::new().max_connections(max).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    Ok(AppState {
        db,
        my_tasks_cache: Arc::new(TtlCache::new(Duration::from_secs(config.cache_ttl_secs))),
        mailer: Arc::new(DevMailer::default()),
        config: Arc::new(config),
    })
}

pub use routes::router;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
