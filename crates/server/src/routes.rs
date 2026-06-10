//! HTTP ルートハンドラー

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::jobs::{create_job, stop_job};
use crate::params::SimParams;
use crate::state::{AppState, Job, JobMeta, JobStatus};

/// POST /api/jobs のリクエストボディ
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobRequest {
    params: SimParams,
    mode: String,
    label: Option<String>,
    threads: Option<usize>,
}

/// POST /api/jobs のレスポンス
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobResponse {
    job_id: Uuid,
}

/// GET /api/jobs の1エントリ
#[derive(Serialize)]
struct JobSummary {
    #[serde(flatten)]
    meta: JobMeta,
    #[serde(flatten)]
    status: JobStatus,
}

/// ルーターを構築して返す。
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs", post(create_job_handler))
        .route("/api/jobs", get(list_jobs_handler))
        .route("/api/jobs/:id", get(get_job_handler))
        .route("/api/jobs/:id/stop", post(stop_job_handler))
        .route("/api/jobs/:id", delete(delete_job_handler))
        .route("/api/jobs/:id/logs", get(logs_handler))
        .route("/api/jobs/:id/results", get(results_handler))
        .route("/api/jobs/:id/diag/:name", get(diag_handler))
        .route("/api/lxcat-files", get(lxcat_files_handler))
}

/// POST /api/jobs — ジョブを作成して実行を開始する
async fn create_job_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let job = create_job(state, req.params, req.mode, req.label, req.threads)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(CreateJobResponse { job_id: job.id }),
    ))
}

/// GET /api/jobs — 全ジョブ一覧を created_at 降順で返す
async fn list_jobs_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut jobs: Vec<JobSummary> = Vec::new();

    for entry in state.jobs.iter() {
        let job = Arc::clone(entry.value());
        let status = job.status.lock().await.clone();
        jobs.push(JobSummary {
            meta: job.meta.clone(),
            status,
        });
    }

    // created_at 文字列は RFC3339 なので辞書順降順 = 新しい順
    jobs.sort_by(|a, b| b.meta.created_at.cmp(&a.meta.created_at));

    Json(jobs)
}

/// GET /api/jobs/:id — 特定ジョブの詳細を返す
async fn get_job_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let job = state
        .jobs
        .get(&id)
        .map(|e| Arc::clone(e.value()))
        .ok_or(StatusCode::NOT_FOUND)?;

    let status = job.status.lock().await.clone();
    Ok(Json(JobSummary {
        meta: job.meta.clone(),
        status,
    }))
}

/// POST /api/jobs/:id/stop — ジョブを停止する
async fn stop_job_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let job = state
        .jobs
        .get(&id)
        .map(|e| Arc::clone(e.value()))
        .ok_or(StatusCode::NOT_FOUND)?;

    stop_job(&job)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/jobs/:id — ジョブを停止し、作業ディレクトリとレジストリから削除する
async fn delete_job_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let job = state
        .jobs
        .get(&id)
        .map(|e| Arc::clone(e.value()))
        .ok_or(StatusCode::NOT_FOUND)?;

    // 実行中なら停止する（失敗しても削除は続ける）
    let _ = stop_job(&job).await;

    // 作業ディレクトリを削除する
    if job.workdir.exists() {
        tokio::fs::remove_dir_all(&job.workdir)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // レジストリから除去する（drop 前に job を解放する）
    drop(job);
    state.jobs.remove(&id);

    Ok(StatusCode::NO_CONTENT)
}

/// SSE finished イベントのペイロード
#[derive(Serialize)]
struct FinishedPayload {
    code: i32,
    stopped: bool,
}

impl FinishedPayload {
    fn from_status(status: &JobStatus) -> Self {
        match status {
            JobStatus::Done { code } => Self {
                code: *code,
                stopped: false,
            },
            JobStatus::Stopped => Self {
                code: -1,
                stopped: true,
            },
            _ => Self {
                code: -2,
                stopped: false,
            },
        }
    }
}

/// GET /api/jobs/:id/logs — SSE でログをストリーミング配信する
async fn logs_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let job = state
        .jobs
        .get(&id)
        .map(|e| Arc::clone(e.value()))
        .ok_or(StatusCode::NOT_FOUND)?;

    // ログバッファをコピーし、同じロックスコープ内で subscribe して gap を防ぐ
    let (past_lines, rx) = {
        let buf = job.log_buf.lock().await;
        let lines = buf.clone();
        let rx = job.log_tx.subscribe();
        (lines, rx)
    };

    // 現在のステータスを確認する
    let current_status = job.status.lock().await.clone();
    let is_terminal = matches!(
        current_status,
        JobStatus::Done { .. } | JobStatus::Failed { .. } | JobStatus::Stopped
    );

    let done = Arc::clone(&job.done);
    let job_arc = Arc::clone(&job);

    let stream = make_log_stream(past_lines, rx, is_terminal, current_status, done, job_arc);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// ログ SSE ストリームを生成する
fn make_log_stream(
    past_lines: Vec<String>,
    rx: tokio::sync::broadcast::Receiver<String>,
    is_terminal: bool,
    terminal_status: JobStatus,
    done: Arc<tokio::sync::Notify>,
    job: Arc<Job>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    // 過去ログのリプレイストリーム
    let replay = stream::iter(
        past_lines
            .into_iter()
            .map(|line| Ok::<Event, Infallible>(Event::default().event("log").data(line))),
    );

    // 終了済みジョブはリプレイ後すぐに finished を送って終了する
    if is_terminal {
        let payload = FinishedPayload::from_status(&terminal_status);
        let json = serde_json::to_string(&payload)
            .unwrap_or_else(|_| String::from("{\"code\":-2,\"stopped\":false}"));
        let finished = stream::once(async move {
            Ok::<Event, Infallible>(Event::default().event("finished").data(json))
        });
        return replay.chain(finished).left_stream();
    }

    // 実行中ジョブ: リプレイ後にライブ受信し、終了後に finished を送る
    enum SsePhase {
        Live {
            rx: tokio::sync::broadcast::Receiver<String>,
            done: Arc<tokio::sync::Notify>,
            job: Arc<Job>,
        },
        Finished {
            job: Arc<Job>,
        },
        End,
    }

    let live_and_finished = stream::unfold(SsePhase::Live { rx, done, job }, |phase| async move {
        match phase {
            SsePhase::Live { mut rx, done, job } => {
                tokio::select! {
                    biased;
                    result = rx.recv() => {
                        match result {
                            Ok(line) => Some((
                                Ok(Event::default().event("log").data(line)),
                                SsePhase::Live { rx, done, job },
                            )),
                            Err(RecvError::Lagged(n)) => {
                                let msg = format!("{n} 行のログを取りこぼしました");
                                Some((
                                    Ok(Event::default().event("log").data(msg)),
                                    SsePhase::Live { rx, done, job },
                                ))
                            }
                            Err(RecvError::Closed) => {
                                Some((
                                    Ok(Event::default().event("log").data(String::new())),
                                    SsePhase::Finished { job },
                                ))
                            }
                        }
                    }
                    _ = done.notified() => {
                        Some((
                            Ok(Event::default().event("log").data(String::new())),
                            SsePhase::Finished { job },
                        ))
                    }
                }
            }
            SsePhase::Finished { job } => {
                // 最終ステータスを読んで finished イベントを1回送る
                let status = job.status.lock().await.clone();
                let payload = FinishedPayload::from_status(&status);
                let json = serde_json::to_string(&payload)
                    .unwrap_or_else(|_| String::from("{\"code\":-2,\"stopped\":false}"));
                Some((
                    Ok(Event::default().event("finished").data(json)),
                    SsePhase::End,
                ))
            }
            SsePhase::End => None,
        }
    });

    replay.chain(live_and_finished).right_stream()
}

/// GET /api/jobs/:id/results  結果ファイル一覧を JSON 配列で返す
async fn results_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let job = state
        .jobs
        .get(&id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "job not found" })),
            )
        })?;

    let names = crate::diagnostics::list_results(&job.workdir);
    Ok(Json(names))
}

/// GET /api/lxcat-files — /data/lxcat 配下のファイル名一覧を返す
async fn lxcat_files_handler(State(state): State<AppState>) -> impl IntoResponse {
    let dir = &state.config.lxcat_dir;
    let mut files: Vec<String> = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    files.push(name.to_string());
                }
            }
        }
    }
    files.sort();
    Json(files)
}

/// GET /api/jobs/:id/diag/:name  診断ファイルをパースして JSON で返す
async fn diag_handler(
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // パストラバーサル対策: ".." やパス区切り文字を含むファイル名は拒否する
    if name.contains("..") || name.contains('/') || name.contains(std::path::MAIN_SEPARATOR) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid file name" })),
        ));
    }

    let job = state
        .jobs
        .get(&id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "job not found" })),
            )
        })?;

    match crate::diagnostics::read_diagnostic(&job.workdir, &name) {
        Ok(diag) => Ok(Json(serde_json::to_value(diag).unwrap())),
        Err(msg) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": msg })),
        )),
    }
}
