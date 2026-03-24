//! Blackjack TVC enclave server binary.

use blackjack::cli::Cli;
use blackjack::router::router;
use blackjack::session::SessionStore;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let store = SessionStore::new();
    SessionStore::spawn_reaper(store.clone());

    let app = router(store);

    let addr = format!("{}:{}", cli.host, cli.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Blackjack server listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
