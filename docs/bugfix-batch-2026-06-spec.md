# バグ修正バッチ 2026-06 仕様書

コードレビューで検出した 3 件のバグと 5 件の軽微な問題をすべて修正する。
対象: `C:\works\Rust-PIC-GUI-Linux`（`crates/server/` と `web/src/`）。

## Fix 1【最重要】SSE finished イベント取りこぼし（Notify の lost-wakeup）

**問題**: `routes.rs` のライブ配信ループ（`make_log_stream` の `SsePhase::Live`）は毎イテレーション `done.notified()` を新規作成し、`biased` select で `rx.recv()` を先に poll する。バックログ処理中は notified が未登録のため、その間の `notify_waiters()`（jobs.rs `execute_job` 末尾）は消失し、以降 `recv()` で永久待ち → クライアントに finished が届かない。

**修正方針**: `tokio::sync::Notify` を **`tokio::sync::watch::Sender<bool>`** に置き換える（watch は状態ベースなので通知が消失しない）。

1. `state.rs` の `Job.done` を `Arc<Notify>` から `watch::Sender<bool>` に変更（フィールド名 `done` は維持してよい）。`Job::new` では `watch::channel(false)` の Sender を保持。
2. `jobs.rs`:
   - `execute_job` 末尾の `notify_waiters()` を `let _ = job.done.send(true);` に変更（ステータス確定後に送る、の順序は維持）。
   - セマフォ取得失敗時の通知も同様に変更。
   - `restore_jobs` では `watch::channel(true)` で作成（復元ジョブは終了済み）。
3. `routes.rs` `logs_handler` / `make_log_stream`:
   - 購読時に `done.subscribe()` で `watch::Receiver<bool>` を取得し、unfold の state（`SsePhase::Live`）に保持して**イテレーション間で使い回す**。
   - select は `biased` のまま `rx.recv()` を先に poll（バックログを送り切ってから finished へ移行するため）。done 側は `done_rx.changed()` を待つ。watch は seen バージョン管理されるため取りこぼしがない。
   - **ついでに既知の小バグも解消**: 現在 Live → Finished の遷移時に**空の log イベントを 1 件送ってから** finished を送っている（フロントは空行フィルタで回避中）。遷移時は空イベントを挟まず、Live ブランチ内で status を読んで finished イベントを直接 yield して End へ進む構造に直す。
   - 接続時に `done_rx.borrow()` が既に true（≒接続とほぼ同時に終了した）場合も正しく finished が送られること。

## Fix 2: cleanup_jobs が Queued ジョブを削除する

`jobs.rs` `cleanup_jobs` の除外条件 `matches!(*s, JobStatus::Running)` を `matches!(*s, JobStatus::Running | JobStatus::Queued)` に変更。コメントも「Running/Queued は削除対象外」に更新。

## Fix 3: シグナル死が「停止」と誤表示される

`jobs.rs` `execute_job` の `Ok(-1)` アーム: 現在は無条件で Stopped にしている。
- status が**既に Stopped**（= `stop_job` がユーザー停止時に設定）の場合のみ Stopped を維持。
- それ以外（status が Running のまま -1 = シグナル死・wait 失敗）は `JobStatus::Failed { message: "プロセスが異常終了しました（シグナル・OOM 等の可能性）".to_string() }` にして warn ログを出す。

## Fix 4: list_jobs_handler の DashMap ガード跨ぎ await

`routes.rs` `list_jobs_handler`: 先に `let snapshot: Vec<Arc<Job>> = state.jobs.iter().map(|e| Arc::clone(e.value())).collect();` でスナップショットを取り、イテレータのガードを解放してから各 job の `status.lock().await` を行う。

## Fix 5: lxcat_path のパストラバーサル検証

`jobs.rs` `create_job` の検証部（waveform 検証と同じ場所）に追加:
- `params.lxcat_path` が Some で空でない場合、`..` / `/` / `\\` のいずれかを含むなら拒否。
- `pub const LXCAT_PATH_INVALID: &str = "invalid lxcat file name";` を定義して `anyhow::anyhow!(LXCAT_PATH_INVALID)` で返す。
- `routes.rs` `create_job_handler` のエラーマッピングで、`CUSTOM_WAVEFORM_DATA_REQUIRED` と同様に `LXCAT_PATH_INVALID` も 400 (BAD_REQUEST) にする。

## Fix 6: meta.json / 一覧 API の customWaveformData 肥大

`jobs.rs` `create_job`: `waveform.csv` を書き出して `params.waveform_file_path` を設定した**後に** `params.custom_waveform_data = None;` をセットしてから `JobMeta::new` に渡す。
- 根拠: 波形の実体は workdir の waveform.csv に永続化済み。meta.json・GET /api/jobs・比較ビューの条件差分に数千点の波形配列を載せる必要はない。
- `SimParams.custom_waveform_data` は `#[serde(default)]` 付きなので、既存 meta.json との互換は保たれる。

## Fix 7: 未使用 import

`jobs.rs` 先頭の `use std::os::unix::process::CommandExt as _;` を削除（`process_group` は tokio Command の固有メソッドのため不要）。コンパイル警告が他にも出る場合は併せて解消。

## Fix 8: SSE 再接続時のログ重複（フロント）

`web/src/lib/api.ts` `subscribeLogs`: EventSource は切断時に自動再接続し、サーバーは接続ごとに全バッファを再送するため、再接続でログが重複表示される。
- `subscribeLogs` に省略可能な `onOpen?: () => void` を追加し、`es.onopen` で呼ぶ。
- `web/src/App.tsx` `handleRun` の購読箇所で `onOpen` に「ログを `[">> ジョブ投入: <jobId>"]` にリセットする」処理を渡す（再接続時にサーバーのリプレイで全行が再送されるため、リセットすれば重複しない）。
- 既存の空行フィルタ（`if (line && line.length > 0)`）は、Fix 1 で空イベントが来なくなっても後方互換のため残してよい。

## 検証

- ホストは Windows のため `cargo check` は不可（Linux 専用 API）。Rust 側のコンパイル検証は発注元が Docker で行う。
- フロントは `cd web && npx tsc --noEmit`（node_modules がなければ `npm ci`）を実行して型エラーがないことを確認すること。

## 禁止事項

- `C:\works\Rust-PIC-GUI`（元ツリー）には触れない。
- API スキーマ（エンドポイント・JSON 形状）は変更しない（customWaveformData が一覧に含まれなくなるのは許容済みの仕様変更）。
- 上記以外のリファクタリングはしない。
