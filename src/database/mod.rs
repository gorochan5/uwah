use anyhow::{Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use std::sync::OnceLock;
use std::str::FromStr;


pub async fn init(token: String) -> Result<()> {
    let config = &crate::config::get();
    let url = &config.database.dir;

    let options = SqliteConnectOptions::from_str(&url)
        .expect("Failed to parse SQLite connection string.")
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .create_if_missing(true);

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("Failed to connect to SQLite.");

    SQLITE_POOL
        .set(db)
        .expect("database::init() was called more than once.");

    Ok(())
}

static SQLITE_POOL: OnceLock<SqlitePool> = OnceLock::new();

pub fn get() -> &'static SqlitePool {
    SQLITE_POOL
        .get()
        .expect("database::init() must be called before get().")
}