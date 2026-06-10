# Rust-PIC Web Linux 設計書

Linux 専用の **Web ブラウザ版** Rust-PIC の実装仕様。Ubuntu/Debian、Docker Compose、systemd での運用を前提とし、計算コア (`crates/rust-pic/`) は Web サーバーから CLI バイナリとして実行する。

## 1. 前提・決定事項

| 項目 | 決定 |
|---|---|
| 利用形態 | 研究室内 LAN・複数人・**認証なし**（誰でもアクセス可、全ジョブ閲覧可） |
| 計算実行 | **サーバー側**。既存 CLI バイナリ `rust-pic` をサブプロセス起動 |
| 計算コア | 無改修（`crates/rust-pic/`） |
| 同時実行 | 複数ジョブ並走。上限はセマフォ＋環境変数 `MAX_CONCURRENT`（既定 4） |
| フロント | Vite + React + TypeScript + Plotly.js |
| 追加機能 | 複数ジョブ結果の**同時可視化（比較ビュー）** |

## 2. アーキテクチャ

```
[ブラウザ(複数)] ──HTTP / SSE──> [Axum サーバー] ──tokio subprocess──> [rust-pic バイナリ]
   React+Plotly                    ジョブ管理            workspaces/<jobId>/result/1d/*.dat
```

サーバーは API と静的フロント（`web/dist`）の両方を配信する。

## 3. Cargo workspace 構成

ルート `Cargo.toml` を workspace 化する。

```
Cargo.toml                # [workspace] members = ["crates/rust-pic", "crates/server"]
crates/
  rust-pic/               # 既存コアを移動 (src/main.rs, lxcat.rs, surface.rs + Cargo.toml)
  server/                 # 新規 Axum サーバー
    src/
      main.rs             # エントリ・ルーター・設定読込
      state.rs            # AppState, JobRegistry
      jobs.rs             # ジョブ生成・サブプロセス実行・ログ配信 (lib.rs から移植)
      params.rs           # SimParams + build_args (lib.rs から移植)
      diagnostics.rs      # read_diagnostic / list_results (lib.rs から移植)
      routes.rs           # ハンドラ
    Cargo.toml
web/                      # 新フロント (Vite)
xsec/                     # LXCat 断面積データ
workspaces/               # 実行時ジョブ領域
```

- この Linux ツリーは Web サーバーとフロントのみを含む。
- `crates/rust-pic/Cargo.toml` は現行 `Cargo.toml` の `[dependencies]`（rand, rand_distr, rayon, bincode, serde）をそのまま使用。バイナリ名は `rust-pic` を維持。

## 4. サーバー設計 (`crates/server`)

### 4.1 依存クレート
```toml
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["fs", "cors", "trace"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
dashmap = "6"
tokio-stream = "0.1"
futures = "0.3"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
```

### 4.2 設定（環境変数）
| 変数 | 既定 | 説明 |
|---|---|---|
| `BIND_ADDR` | `0.0.0.0:8090` | 待受アドレス（LAN 公開） |
| `RUST_PIC_BIN` | `target/release/rust-pic` | 計算バイナリパス |
| `WORKSPACES_DIR` | `./workspaces` | ジョブ作業領域ルート |
| `WEB_DIST` | `./web/dist` | 静的フロント配信元 |
| `LXCAT_DIR` | `/data/lxcat` | LXCat 断面積ファイルのディレクトリ |
| `MAX_CONCURRENT` | `4` | 同時実行ジョブ上限（`tokio::sync::Semaphore`） |
| `MAX_JOBS` | `50` | 保持ジョブ数の上限 |

### 4.3 状態管理
```rust
struct AppState {
    jobs: DashMap<Uuid, Arc<Job>>,
    sem: Arc<Semaphore>,        // 同時実行上限
    config: Config,
}

struct Job {
    id: Uuid,
    meta: JobMeta,                          // 条件サマリ（一覧/比較用）
    status: Mutex<JobStatus>,               // Queued|Running|Done{code}|Failed{msg}|Stopped
    log_tx: broadcast::Sender<String>,      // SSE 配信元（過去ログはバッファ保持）
    log_buf: Mutex<Vec<String>>,            // 後続接続へのリプレイ用（上限 MAX_LOG=2000）
    child: Mutex<Option<Child>>,            // 停止用ハンドル
    workdir: PathBuf,                        // workspaces/<id>/
}
```

### 4.4 ジョブのライフサイクル
1. `POST /api/jobs` 受信 → `Uuid` 発行、`workspaces/<id>/` 作成、`meta.json`（params＋作成時刻）保存、`Job` を registry 登録、`Queued` で即レス。
2. tokio タスク起動 → `sem.acquire()` 待機 → `Running`。
3. `mode` に従い `build_args` で引数生成し `rust-pic` をサブプロセス実行（cwd=workdir）。Linux ではプロセスグループを分離し、停止時にジョブ単位で終了させる。`init_measure` は `0`(init) → `<cycles> m`(measure) の 2 段。stdout/stderr を行単位で `log_tx` へ送出＋`log_buf` へ蓄積。
4. 終了 → `Done{code}`/`Failed`/`Stopped`。`log_tx` をドロップして SSE を閉じる。
5. `init` 系は実行前に `picdata.bin` と `result/1d` を削除（`clean_for_init` を移植）。

> 注: サーバーは `tokio::process::Command` + 非同期読取で `rust-pic` を実行する。ロジック（引数組立・段階実行）は計算コア側と等価に保つ。

### 4.5 データモデル

`SimParams`（`lib.rs` の構造体をそのまま流用、`#[serde(rename_all="camelCase")]`）。
`load_waveform_file`（ローカルパス読込）は廃止し、**波形はフロントからアップロード**して `custom_waveform_data`（`Vec<(f64,f64)>`）として `params` に含める。

```rust
struct JobMeta {
    id: Uuid,
    created_at: String,        // RFC3339
    label: Option<String>,     // 任意の表示名
    params: SimParams,         // 比較ビューでの条件差分表示に使用
}

#[serde(tag="status", rename_all="lowercase")]
enum JobStatus { Queued, Running, Done{code:i32}, Failed{message:String}, Stopped }
```

`Diagnostic`（`lib.rs` と同一）:
```rust
#[serde(tag="kind", rename_all="lowercase")]
enum Diagnostic {
    Columns { columns: Vec<Vec<f64>>, labels: Option<Vec<String>> },
    Matrix  { matrix: Vec<Vec<f64>> },
    Text    { text: String },
}
```

## 5. API スキーマ

| メソッド・パス | リクエスト | レスポンス | 備考 |
|---|---|---|---|
| `POST /api/jobs` | `{ params: SimParams, mode: RunMode, label?: string, threads?: number }` | `{ jobId: string }` | ジョブ投入。即レス |
| `GET /api/jobs` | — | `JobMeta[]`（status 付き、created_at 降順） | 一覧・比較選択用 |
| `GET /api/jobs/{id}` | — | `{ meta: JobMeta, status: JobStatus }` | 状態取得 |
| `POST /api/jobs/{id}/stop` | — | `204` | 実行中プロセスを kill |
| `DELETE /api/jobs/{id}` | — | `204` | 停止＋workspace 削除 |
| `GET /api/jobs/{id}/logs` | — | `text/event-stream` | SSE。接続時に `log_buf` をリプレイ後、ライブ配信。終了時 `event: finished` |
| `GET /api/jobs/{id}/results` | — | `string[]`（`result/1d` のファイル名） | |
| `GET /api/jobs/{id}/diag/{name}` | — | `Diagnostic` | `.dat`/`info.txt` をパース |

- `RunMode = "init" | "run" | "measure" | "init_measure"`。
- CORS: 開発時 `vite` (5173) ⇄ サーバー (8090) を許可。本番は同一オリジン配信。
- SSE の `finished` イベント payload: `{ code: number, stopped: boolean }`（`onFinished` 相当）。

### SSE 詳細
- レスポンスは `Sse<impl Stream<Item=Event>>`。
- 接続直後に `log_buf` の全行を `event: log` で送出 → `log_tx.subscribe()` のライブ行を継続送出。
- ジョブ終了で stream を close する前に `event: finished` を 1 回送る。

## 6. フロント設計 (`web/`)

Vite + React + TypeScript + Plotly.js。`lib/api.ts` は **fetch + EventSource** ベースで、Axum の `/api/*` と SSE に接続する。

### 画面構成
- **計算条件**: 既存 `ParamForm`/`params.ts`/`types.ts`/`WaveformPreview` を参考に再構築。波形は file input でアップロード。`workdir`/`exePath` 入力は廃止。
- **実行**: ジョブ投入 → `EventSource(/api/jobs/{id}/logs)` でログ表示。停止ボタン。
- **ジョブ一覧**: `GET /api/jobs`。状態バッジ、ラベル/条件サマリ、選択チェックボックス。
- **可視化**: 単一ジョブ表示（既存 `Visualize`/`diagnostics.ts`/`lib/plot.ts` のカタログ・描画ロジックを流用）。
- **比較ビュー（新規）**: 一覧で選択した複数ジョブについて、各ジョブの `diag/{name}` を取得し
  - line 系: 同一 Plotly figure にジョブごとトレースを重畳（凡例＝ラベル/条件差分）。
  - heatmap 系: ジョブごとに subplot をグリッド並列表示。
  - 条件差分は `params` を比較して自動でラベル化（例 `pressure=10Pa / 20Pa`）。

`diagnostics.ts`（診断カタログ）と `plot.ts`（Plotly 描画）はロジックをほぼ再利用可能。

## 7. Linux 運用

### 7.1 ローカルビルドと起動

```bash
./scripts/build.sh
./scripts/run.sh
```

### 7.2 開発起動

```bash
./scripts/dev.sh
```

`scripts/dev.sh` は `cargo run -p server` と `npm run dev` を並列起動し、Ctrl-C で両方を停止する。

### 7.3 Docker

```bash
docker compose up --build
```

`./workspaces` は `/app/workspaces`、`./xsec` は `/data/lxcat` にマウントする。

### 7.4 systemd

`deploy/rust-pic-web.service` は `/opt/rust-pic` 配置の例。release バイナリ、`web/dist`、`xsec/`、`workspaces/` を同じプレフィックス配下に置き、`systemctl enable --now rust-pic-web` で常駐起動する。

## 8. 検証

- Linux または Docker 環境で `cargo check` または `cargo build --release -p rust-pic -p server` を実行する。
- フロントは `cd web && npm ci && npx tsc --noEmit` で TypeScript を検証する。
- 静的配信まで確認する場合は `cd web && npm run build` 後、`./scripts/run.sh` で `http://localhost:8090` を開く。
