//! アプリケーション状態とジョブデータ構造

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch, Mutex, Semaphore};
use uuid::Uuid;

use crate::params::SimParams;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: std::net::SocketAddr,
    pub rust_pic_bin: Option<PathBuf>,
    pub workspaces_dir: PathBuf,
    pub web_dist: PathBuf,
    pub lxcat_dir: PathBuf,
    pub max_concurrent: usize,
    /// registry に保持するジョブの最大数（超過分は古い完了ジョブから削除）
    pub max_jobs: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::Context;

        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8090".to_string())
            .parse()
            .context("BIND_ADDR parse failed")?;

        let rust_pic_bin = std::env::var_os("RUST_PIC_BIN").map(PathBuf::from);
        let workspaces_dir = std::env::var_os("WORKSPACES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./workspaces"));
        let web_dist = std::env::var_os("WEB_DIST")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./web/dist"));
        let lxcat_dir = std::env::var_os("LXCAT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/data/lxcat"));
        let max_concurrent = std::env::var("MAX_CONCURRENT")
            .unwrap_or_else(|_| "4".to_string())
            .parse()
            .context("MAX_CONCURRENT parse failed")?;
        let max_jobs = std::env::var("MAX_JOBS")
            .unwrap_or_else(|_| "50".to_string())
            .parse()
            .context("MAX_JOBS parse failed")?;

        Ok(Self {
            bind_addr,
            rust_pic_bin,
            workspaces_dir,
            web_dist,
            lxcat_dir,
            max_concurrent,
            max_jobs,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobMeta {
    pub id: Uuid,
    pub created_at: String,
    pub label: Option<String>,
    pub params: SimParams,
}

impl JobMeta {
    pub fn new(id: Uuid, label: Option<String>, params: SimParams) -> Self {
        let created_at: DateTime<Utc> = Utc::now();
        Self {
            id,
            created_at: created_at.to_rfc3339(),
            label,
            params,
        }
    }
}

/// ジョブ実行状態。serde tag でバリアント名を "status" フィールドに出力する。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done { code: i32 },
    Failed { message: String },
    Stopped,
}

pub struct Job {
    pub id: Uuid,
    pub meta: JobMeta,
    /// 現在の実行状態（Mutex 保護）
    pub status: Mutex<JobStatus>,
    /// ログ行ブロードキャスト送信端（購読はいつでも可能）
    pub log_tx: broadcast::Sender<String>,
    /// ログ行バッファ（最大 2000 行、Mutex 保護）
    pub log_buf: Mutex<Vec<String>>,
    /// 実行中の子プロセス（停止時に kill する）
    pub child: Mutex<Option<tokio::process::Child>>,
    pub workdir: PathBuf,
    /// ジョブ完了通知（false→true。watch のため購読タイミングによらず取りこぼさない）
    pub done: watch::Sender<bool>,
}

impl Job {
    pub fn new(id: Uuid, meta: JobMeta, workdir: PathBuf) -> Self {
        let (log_tx, _) = broadcast::channel(64);
        let (done, _) = watch::channel(false);
        Self {
            id,
            meta,
            status: Mutex::new(JobStatus::Queued),
            log_tx,
            log_buf: Mutex::new(Vec::new()),
            child: Mutex::new(None),
            workdir,
            done,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<DashMap<Uuid, Arc<Job>>>,
    pub sem: Arc<Semaphore>,
    pub config: Config,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let max_concurrent = config.max_concurrent;
        Self {
            jobs: Arc::new(DashMap::new()),
            sem: Arc::new(Semaphore::new(max_concurrent)),
            config,
        }
    }
}
