use anyhow::Result;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use std::str::FromStr;
use std::sync::OnceLock;

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
    .pragma("cache_size", "-64000")
    .pragma("mmap_size", "268435456")
    .pragma("temp_store", "MEMORY")
    .pragma("auto_vacuum", "INCREMENTAL")
    .create_if_missing(true);

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    tracing::info!("Running database migrations on uwah.db");
    if let Err(err) = sqlx::migrate!("./migrations").run(&db).await {
        tracing::error!(
            error = %err,
            "Database migration failed! If a migration file was edited after being applied, \
            rebuild the binary with 'cargo build' to update the compiled-in checksum, \
            or reset the dev database."
        );
        return Err(err.into());
    }
    tracing::info!("Database migrations applied successfully");

    let _ = SQLITE_POOL.set(db.clone());

    Ok(db)
}

pub fn get() -> &'static SqlitePool {
    SQLITE_POOL
        .get()
        .expect("database::init() must be called before database::get()")
}
