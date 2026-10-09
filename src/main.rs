use adsb::{app, AppState, FlightStore, ReportStore};
use std::{env, net::SocketAddr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://adsb:adsb@localhost:5432/adsb?sslmode=disable".into());
    adsb::parse_database_url(&database_url).map_err(std::io::Error::other)?;
    let store = FlightStore::connect(&database_url).await?;
    store.migrate().await?;
    let reports = ReportStore::new(store.pool().clone());
    let address: SocketAddr = env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("ADS-B listening on {address}");
    axum::serve(listener, app(AppState { store, reports }))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for terminate")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {}, }
}
