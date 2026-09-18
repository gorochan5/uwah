use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use std::sync::OnceLock;
use std::str::FromStr;

static SQLITE_POOL: OnceLock<SqlitePool> = OnceLock::new();

pub async fn init() -> Result<SqlitePool> {
    let config = crate::config::get();
    let url = &config.database.dir;

    let conn_str = if url.starts_with("sqlite:") {
        url.clone()
    } else {
        format!("sqlite://{}", url)
    };

    let options = match SqliteConnectOptions::from_str(&conn_str) {
        Ok(opt) => opt,
        Err(_) => SqliteConnectOptions::new().filename(url),
    }
    .foreign_keys(true)
    .journal_mode(SqliteJournalMode::Wal)
    .synchronous(SqliteSynchronous::Normal)
    .busy_timeout(std::time::Duration::from_secs(30))
    .create_if_missing(true);

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    let _ = SQLITE_POOL.set(db.clone());

    Ok(db)
}

pub fn get() -> &'static SqlitePool {
    SQLITE_POOL
        .get()
        .expect("database::init() must be called before database::get()")
}