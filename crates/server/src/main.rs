//! Rust-PIC Web サーバー

mod diagnostics;
mod jobs;
mod params;
mod routes;
mod state;

use anyhow::Context;
use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

use crate::state::{AppState, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ設定を環境変数から初期化する。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("server=info,tower_http=info")
            }),
        )
        .init();

    let config = Config::from_env().context("設定の読み込みに失敗しました")?;
    info!(
        addr = %config.bind_addr,
        rust_pic_bin = ?config.rust_pic_bin,
        workspaces_dir = %config.workspaces_dir.display(),
        web_dist = %config.web_dist.display(),
        max_concurrent = config.max_concurrent,
        "サーバー設定を読み込みました"
    );

    // workspaces ディレクトリが存在しない場合は作成する
    tokio::fs::create_dir_all(&config.workspaces_dir)
        .await
        .context("workspaces ディレクトリの作成に失敗しました")?;

    let state = AppState::new(config.clone());

    // 既存の workspaces からジョブを復元し、上限超過分を掃除する
    if let Err(e) = jobs::restore_jobs(&state).await {
        tracing::warn!("ジョブ復元に失敗しました: {e}");
    }
    jobs::cleanup_jobs(&state).await;
    info!(
        jobs = state.jobs.len(),
        max_jobs = config.max_jobs,
        "ジョブを復元しました"
    );

    // 開発用に許可的な CORS を設定する
    let cors = CorsLayer::permissive();

    let app = Router::new()
        .route("/api/health", get(health))
        .merge(routes::api_router())
        .layer(cors)
        .fallback_service(ServeDir::new(&config.web_dist))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    info!(addr = %config.bind_addr, "サーバーを起動しました");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}
