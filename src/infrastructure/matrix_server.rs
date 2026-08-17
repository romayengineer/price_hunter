//! Serves the product × provider price matrix over HTTP for local UIs.

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

use crate::application::matrix as matrix_service;
use crate::domain::model::Matrix;
use crate::infrastructure::store::Store;

/// Serves the product × provider price matrix over HTTP for local UIs.
///
/// Binds to `127.0.0.1:<port>` only (default 8091, override with
/// `PRICE_HUNTER_MATRIX_PORT`) and queries PocketBase per request. The
/// blocking PocketBase call runs on a blocking thread so it never stalls the
/// async server.
pub async fn serve(store: Store) -> anyhow::Result<()> {
    let port = std::env::var("PRICE_HUNTER_MATRIX_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8091);
    let state = Arc::new(store);
    let app = Router::new()
        .route("/health", get(health))
        .route("/matrix", get(matrix))
        .with_state(state);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("could not bind matrix server on {addr}"))?;
    println!("Matrix server listening on http://{addr} (GET /matrix)");
    axum::serve(listener, app)
        .await
        .context("matrix server failed")
}

async fn health() -> &'static str {
    "ok"
}

async fn matrix(State(store): State<Arc<Store>>) -> Result<Json<Matrix>, (StatusCode, String)> {
    let store = store.clone();
    let matrix = tokio::task::spawn_blocking(move || matrix_service::matrix(&*store))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
    Ok(Json(matrix))
}
