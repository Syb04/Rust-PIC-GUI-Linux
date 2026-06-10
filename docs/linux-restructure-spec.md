# Linux 向け再構成 仕様書

対象ディレクトリ: `C:\works\Rust-PIC-GUI-Linux`（Windows 版 `C:\works\Rust-PIC-GUI` からのクリーンコピー。旧 Tauri 版 `gui/`、ビルド成果物、`.bak`/`.orig`/`.tmp` は除外済み）。

## 目的

このツリーは **Linux 専用**（Ubuntu/Debian 想定、Docker デプロイ中心）として再構成する。Windows サポートは不要。

## 構成（変更しない）

- Cargo workspace: `crates/rust-pic`（計算コア CLI）、`crates/server`（Axum Web サーバー）
- `web/`: Vite + React + TypeScript フロントエンド
- `Dockerfile`（マルチステージ: rust:1-bookworm → node → debian-slim）、`docker-compose.yml`
- ポート 8090、環境変数 `BIND_ADDR` / `RUST_PIC_BIN` / `LXCAT_DIR` / `MAX_CONCURRENT` / `MAX_JOBS`

## タスク

### 1. `crates/server/src/jobs.rs` — Windows 固有コードの除去

- `resolve_bin()`: `cfg!(windows)` 分岐を削除し、常に `"rust-pic"`（拡張子なし）を返す。
- `run_step()` 内の `#[cfg(windows)]` ブロック（`CREATE_NO_WINDOW` / `creation_flags`）を削除。
- 代わりに Unix 向け改善: `cmd.process_group(0)` を設定し（tokio 1.x の `Command::process_group`、Unix 専用 API）、停止時に子プロセスごと確実に kill できるようにする。`#[cfg(unix)]` ガードは不要（Linux 専用ツリーのため直接呼んでよいが、tokio の API シグネチャ上 cfg が必要ならガード付きで可）。

### 2. ソース全体の Windows 残滓チェック

- `crates/` と `web/src/` を grep し、`\.exe`・`cfg(windows)`・バックスラッシュパス・`os::windows` が残っていないことを確認。あれば除去。
- `crates/server/src/main.rs` 先頭の UTF-8 BOM（`\u{FEFF}`）を除去。他ファイルにも BOM があれば同様に除去。

### 3. 改行コードの統一

- ルートに `.gitattributes` を新規作成: `* text=auto eol=lf` ＋ バイナリ拡張子の除外指定。
- `crates/**/*.rs`、`web/src/**`、`*.toml`、`*.yml`、`Dockerfile`、`*.md`、`*.sh` の改行を CRLF → LF に変換する。

### 4. Linux 用スクリプト追加（`scripts/`、すべて bash・LF・shebang `#!/usr/bin/env bash`・`set -euo pipefail`）

- `scripts/build.sh`: `cargo build --release -p rust-pic -p server` ＋ `cd web && npm ci && npm run build`
- `scripts/run.sh`: ビルド済み前提でサーバー起動（`BIND_ADDR` 既定 `0.0.0.0:8090`、`RUST_PIC_BIN` 既定 `target/release/rust-pic`、`WEB_DIST` があればそれを使用）
- `scripts/dev.sh`: バックエンド（`cargo run -p server`）とフロント（`cd web && npm run dev`）を並行起動し、Ctrl-C で両方停止

### 5. systemd ユニット例の追加

- `deploy/rust-pic-web.service`: `/opt/rust-pic` 配置想定の systemd unit（`Environment=BIND_ADDR=0.0.0.0:8090` 等、`Restart=on-failure`）。コメントでインストール手順を簡記。

### 6. README.md の Linux 向け更新

- 「必要環境」を Linux（Ubuntu/Debian、rustup、Node.js 20+、または Docker のみ）に書き換え。
- `target/release/rust-pic[.exe]` の `[.exe]` 表記を削除。
- 「開発手順」に `scripts/*.sh` の利用を追記。systemd デプロイ節を追加。
- `docs/web-migration-spec.md` の `[.exe]` 表記も同様に修正。
- 旧デスクトップ版（Tauri/`gui/`）への言及はこのツリーには存在しない旨に修正（または節を削除）。

## 検証（重要）

- **このマシンは Windows ホスト**。Unix 専用 API を入れるため、ホストでの `cargo check` は通らなくてよい。
- 検証は WSL (Ubuntu) または Docker で行う:
  - 可能なら `wsl -d Ubuntu` 内で `cargo check`（rustup がなければスキップ可）
  - フロントは `cd web && npx tsc --noEmit` がホストでも検証可能（node_modules が無ければ `npm ci`）
- Docker フルビルド検証は発注元（Claude）側で実施するので不要。

## 禁止事項

- `C:\works\Rust-PIC-GUI`（元ツリー）には一切触れない。作業はすべて `C:\works\Rust-PIC-GUI-Linux` 内で行う。
- API・パラメータスキーマ・フロントのロジックは変更しない（Linux 化に必要な箇所のみ）。
