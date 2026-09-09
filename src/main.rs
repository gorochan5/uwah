use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::{env, error::Error, net::SocketAddr, sync::Arc};
use crate::database::{self, init};
mod log;
mod models;
mod repository;
mod routes;
mod security;
mod config;
mod api;
mod database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {

    let args = env::args().collect::<Vec<String>>();
    match args.get(1).

    init(token)

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    db::initialize(&db).await?;
    auth::ensure_admin_token(&db).await?;

    let app = routes::router(state);

    let address: SocketAddr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:5855".to_owned())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;

    eprintln!("kronos_kpis is listening on http://{address}");
    eprintln!("On first launch, the generated administrator token is saved in token.txt.");
    axum::serve(listener, app).await?;

    Ok(())
}
