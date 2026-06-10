//! ジョブの生成・実行・停止ロジック

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::params::{build_args, SimParams};
use crate::state::{AppState, Job, JobMeta, JobStatus};

pub const CUSTOM_WAVEFORM_DATA_REQUIRED: &str = "custom mode requires waveform data";
pub const LXCAT_PATH_INVALID: &str = "invalid lxcat file name";

/// ログ行を broadcast チャンネルに送り、バッファにも保持する（上限 2000 行）
async fn push_log(job: &Arc<Job>, line: String) {
    let _ = job.log_tx.send(line.clone());
    let mut buf = job.log_buf.lock().await;
    if buf.len() >= 2000 {
        buf.remove(0);
    }
    buf.push(line);
}

/// 実行前に作業ディレクトリの picdata.bin と result/1d を削除する
async fn clean_for_init(workdir: &PathBuf) -> anyhow::Result<()> {
    let picdata = workdir.join("picdata.bin");
    if picdata.exists() {
        tokio::fs::remove_file(&picdata)
            .await
            .map_err(|e| anyhow::anyhow!("picdata.bin 削除失敗: {e}"))?;
    }
    let result_dir = workdir.join("result").join("1d");
    if result_dir.exists() {
        tokio::fs::remove_dir_all(&result_dir)
            .await
            .map_err(|e| anyhow::anyhow!("result/1d 削除失敗: {e}"))?;
    }
    Ok(())
}

/// 計算バイナリのパスを決定する
fn resolve_bin(config: &crate::state::Config) -> PathBuf {
    if let Some(bin) = &config.rust_pic_bin {
        return bin.clone();
    }
    PathBuf::from("target").join("release").join("rust-pic")
}

/// 1 回分のサブプロセスを起動し、stdout/stderr をログに流す。終了コードを返す。
async fn run_step(
    job: &Arc<Job>,
    bin: &PathBuf,
    cycle_arg: &str,
    measure: bool,
    threads: Option<usize>,
    lxcat_dir: &Path,
) -> anyhow::Result<i32> {
    let args = build_args(&job.meta.params, cycle_arg, measure, lxcat_dir);
    push_log(
        job,
        format!(">> 実行: {} {}", bin.display(), args.join(" ")),
    )
    .await;

    let mut cmd = Command::new(bin);
    cmd.args(&args)
        .current_dir(&job.workdir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    cmd.process_group(0);

    if let Some(t) = threads {
        if t > 0 {
            cmd.env("RAYON_NUM_THREADS", t.to_string());
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("バイナリ起動失敗 ({}): {e}", bin.display()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *job.child.lock().await = Some(child);

    let job_stdout = Arc::clone(job);
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                push_log(&job_stdout, line).await;
            }
        }
    });

    let job_stderr = Arc::clone(job);
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                push_log(&job_stderr, line).await;
            }
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let code = {
        let mut guard = job.child.lock().await;
        match guard.take() {
            Some(mut c) => c.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1),
            None => -1,
        }
    };

    Ok(code)
}

/// バックグラウンドでジョブを実行するタスク
async fn execute_job(
    state: AppState,
    job: Arc<Job>,
    mode: String,
    threads: Option<usize>,
    sem: Arc<Semaphore>,
) {
    let _permit = match sem.acquire().await {
        Ok(p) => p,
        Err(e) => {
            error!(job_id = %job.id, "セマフォ取得失敗: {e}");
            *job.status.lock().await = JobStatus::Failed {
                message: format!("セマフォ取得失敗: {e}"),
            };
            // 失敗時も完了通知を送る
            job.done.send_replace(true);
            return;
        }
    };

    info!(job_id = %job.id, mode = %mode, "ジョブ実行開始");
    *job.status.lock().await = JobStatus::Running;

    let bin = resolve_bin(&state.config);
    let lxcat_dir = state.config.lxcat_dir.clone();
    let cycles = job.meta.params.cycles.to_string();

    let result: anyhow::Result<i32> = match mode.as_str() {
        "init" => match clean_for_init(&job.workdir).await {
            Err(e) => Err(e),
            Ok(_) => run_step(&job, &bin, "0", false, threads, &lxcat_dir).await,
        },
        "run" => run_step(&job, &bin, &cycles, false, threads, &lxcat_dir).await,
        "measure" => run_step(&job, &bin, &cycles, true, threads, &lxcat_dir).await,
        "init_measure" => match clean_for_init(&job.workdir).await {
            Err(e) => Err(e),
            Ok(_) => match run_step(&job, &bin, "0", false, threads, &lxcat_dir).await {
                Ok(0) => run_step(&job, &bin, &cycles, true, threads, &lxcat_dir).await,
                Ok(code) => Ok(code),
                Err(e) => Err(e),
            },
        },
        other => Err(anyhow::anyhow!("不明な実行モード: {other}")),
    };

    match result {
        Ok(-1) => {
            // -1 は「stop_job が子プロセスを取り上げた」か「シグナル死（OOM 等）」。
            // ユーザー停止なら stop_job が既に Stopped を設定している。
            let mut status = job.status.lock().await;
            if matches!(*status, JobStatus::Stopped) {
                info!(job_id = %job.id, "ジョブ停止");
            } else {
                warn!(job_id = %job.id, "プロセスが異常終了しました（シグナル等）");
                *status = JobStatus::Failed {
                    message: "プロセスが異常終了しました（シグナル・OOM 等の可能性）".to_string(),
                };
            }
        }
        Ok(code) => {
            *job.status.lock().await = JobStatus::Done { code };
            info!(job_id = %job.id, exit_code = code, "ジョブ完了");
        }
        Err(e) => {
            let message = e.to_string();
            warn!(job_id = %job.id, error = %message, "ジョブ失敗");
            *job.status.lock().await = JobStatus::Failed { message };
        }
    }

    // ステータス確定後に SSE 購読者へ終了を通知する
    job.done.send_replace(true);
}

/// ジョブを生成してレジストリに登録し、バックグラウンドで実行を開始する。
pub async fn create_job(
    state: AppState,
    mut params: SimParams,
    mode: String,
    label: Option<String>,
    threads: Option<usize>,
) -> anyhow::Result<Arc<Job>> {
    // workdir 作成前に検証し、不正リクエストで空ディレクトリが残らないようにする
    if let Some(name) = params.lxcat_path.as_deref() {
        if !name.is_empty() && (name.contains("..") || name.contains('/') || name.contains('\\')) {
            anyhow::bail!(LXCAT_PATH_INVALID);
        }
    }

    let waveform_csv = if params.voltage_mode == "custom" {
        let waveform_data = params
            .custom_waveform_data
            .as_ref()
            .filter(|data| !data.is_empty())
            .ok_or_else(|| anyhow::anyhow!(CUSTOM_WAVEFORM_DATA_REQUIRED))?;

        let mut csv = String::new();
        for (time, voltage) in waveform_data {
            csv.push_str(&format!("{},{}\n", time, voltage));
        }
        Some(csv)
    } else {
        None
    };

    let id = Uuid::new_v4();
    let workdir = state.config.workspaces_dir.join(id.to_string());

    tokio::fs::create_dir_all(&workdir)
        .await
        .map_err(|e| anyhow::anyhow!("作業ディレクトリ作成失敗 ({}): {e}", workdir.display()))?;

    if let Some(csv) = waveform_csv {
        let waveform_file = "waveform.csv";
        tokio::fs::write(workdir.join(waveform_file), csv)
            .await
            .map_err(|e| anyhow::anyhow!("waveform.csv 書き込み失敗: {e}"))?;
        params.waveform_file_path = Some(waveform_file.to_string());
        // 実体は waveform.csv に永続化済み。meta.json と一覧 API を肥大化させない
        params.custom_waveform_data = None;
    }

    let meta = JobMeta::new(id, label, params);
    let meta_json = serde_json::to_string_pretty(&meta)
        .map_err(|e| anyhow::anyhow!("meta.json シリアライズ失敗: {e}"))?;
    tokio::fs::write(workdir.join("meta.json"), meta_json)
        .await
        .map_err(|e| anyhow::anyhow!("meta.json 書き込み失敗: {e}"))?;

    let job = Arc::new(Job::new(id, meta, workdir));
    state.jobs.insert(id, Arc::clone(&job));

    // 上限超過分の古い完了ジョブを掃除する
    cleanup_jobs(&state).await;

    let sem = Arc::clone(&state.sem);
    let job_clone = Arc::clone(&job);
    tokio::spawn(execute_job(state, job_clone, mode, threads, sem));

    Ok(job)
}

/// 実行中のジョブを停止する。
pub async fn stop_job(job: &Job) -> anyhow::Result<()> {
    let mut guard = job.child.lock().await;
    if let Some(mut child) = guard.take() {
        child
            .kill()
            .await
            .map_err(|e| anyhow::anyhow!("プロセス停止失敗: {e}"))?;
        *job.status.lock().await = JobStatus::Stopped;
    }
    Ok(())
}

/// 起動時に workspaces/ 配下の既存ジョブを復元する。
/// meta.json が読めないディレクトリはスキップ（警告ログ）。
pub async fn restore_jobs(state: &AppState) -> anyhow::Result<()> {
    let mut read_dir = match tokio::fs::read_dir(&state.config.workspaces_dir).await {
        Ok(d) => d,
        Err(e) => {
            warn!("workspaces ディレクトリの読み取り失敗、復元をスキップ: {e}");
            return Ok(());
        }
    };

    while let Some(entry) = read_dir.next_entry().await? {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let meta_path = entry_path.join("meta.json");
        let meta: JobMeta = match tokio::fs::read_to_string(&meta_path).await {
            Err(e) => {
                warn!(path = %meta_path.display(), "meta.json 読み取り失敗、スキップ: {e}");
                continue;
            }
            Ok(json) => match serde_json::from_str(&json) {
                Err(e) => {
                    warn!(path = %meta_path.display(), "meta.json パース失敗、スキップ: {e}");
                    continue;
                }
                Ok(m) => m,
            },
        };

        let id = meta.id;
        // result/1d が存在すれば完了、なければ未完了として復元
        let status = if entry_path.join("result").join("1d").exists() {
            JobStatus::Done { code: 0 }
        } else {
            JobStatus::Failed {
                message: "未完了（再起動前に中断）".to_string(),
            }
        };

        let (log_tx, _) = tokio::sync::broadcast::channel(64);
        // 復元ジョブは終了済みなので done=true で作成する
        let (done, _) = tokio::sync::watch::channel(true);

        let job = Arc::new(crate::state::Job {
            id,
            meta,
            status: tokio::sync::Mutex::new(status),
            log_tx,
            log_buf: tokio::sync::Mutex::new(Vec::new()),
            child: tokio::sync::Mutex::new(None),
            workdir: entry_path,
            done,
        });

        state.jobs.insert(id, job);
        info!(job_id = %id, "ジョブを復元しました");
    }

    Ok(())
}

/// registry のジョブ数が max_jobs を超えていたら、古い完了済みジョブを削除する。
/// Running / Queued（実行待ち）状態のジョブは削除対象外。
pub async fn cleanup_jobs(state: &AppState) {
    let max_jobs = state.config.max_jobs;
    if state.jobs.len() <= max_jobs {
        return;
    }

    // Running 以外のジョブを created_at 昇順で収集
    let mut candidates: Vec<(String, Uuid)> = state
        .jobs
        .iter()
        .filter_map(|entry| {
            let job = entry.value().clone();
            // Running/Queued は非同期ロックが必要なため try_lock で判定
            let is_active = job
                .status
                .try_lock()
                .map(|s| matches!(*s, JobStatus::Running | JobStatus::Queued))
                .unwrap_or(true); // ロック取得失敗 = 実行中とみなす
            if is_active {
                None
            } else {
                Some((job.meta.created_at.clone(), job.id))
            }
        })
        .collect();

    // 古い順にソート
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    let excess = state.jobs.len().saturating_sub(max_jobs);
    for (_, id) in candidates.iter().take(excess) {
        if let Some((_, job)) = state.jobs.remove(id) {
            let workdir = job.workdir.clone();
            if let Err(e) = tokio::fs::remove_dir_all(&workdir).await {
                warn!(job_id = %id, path = %workdir.display(), "workspace 削除失敗: {e}");
            } else {
                info!(job_id = %id, "古いジョブを削除しました");
            }
        }
    }
}
