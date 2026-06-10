# Rust-PIC

高周波プラズマ向け **1D3V PIC/MCC シミュレーション**を、Web ブラウザから計算条件の設定・実行・結果可視化・複数条件の比較まで行えるアプリケーションです。

このツリーは Linux 専用です。Ubuntu/Debian 上の直接起動、Docker Compose、または systemd による Axum サーバー運用を想定しています。

## 構成要素

- **Rust 計算コア** (`crates/rust-pic/`) — CLI 引数対応・Rayon 並列の 1D3V 静電 PIC/MCC コア
- **Axum Web サーバー** (`crates/server/`) — 計算バイナリをジョブ単位でサブプロセス起動し、ログを SSE で配信、診断結果を JSON で返す。フロントの静的配信も担う
- **React フロント** (`web/`) — Vite + React + TypeScript + Plotly.js。計算条件 / 実行 / ジョブ一覧 / 可視化・比較の 4 画面

## 主な機能

- ブラウザから計算条件を設定して計算を実行
- **シミュレーションログのリアルタイム表示**（Server-Sent Events）
- 診断結果の可視化（密度・EEPF・FED・断面積・収束、XT 時空間マップ）
- **複数ジョブの同時比較**（条件違いの結果を重ね描き／グリッド表示、条件差分を自動ラベル化）
- LXCat ガスモデルのサポート（`xsec/` に配置したファイルをブラウザから選択）
- ジョブ単位の作業領域分離・複数ジョブの並列実行（上限はセマフォで制御）
- サーバー再起動後もジョブ一覧・結果を復元

## アーキテクチャ

```
[ブラウザ(複数)] ──HTTP / SSE──> [Axum サーバー] ──subprocess──> [rust-pic バイナリ]
   React + Plotly                  ジョブ管理              workspaces/<jobId>/result/1d/*.dat
```

- 各ジョブは `workspaces/<jobId>/` に隔離され、`meta.json`（計算条件）と `result/1d/*.dat`（診断出力）を保持します。
- サーバーは API（`/api/*`）とビルド済みフロント（`web/dist`）の両方を配信します。

## ディレクトリ構成

```
Rust-PIC-GUI-Linux/
├── Cargo.toml              # Cargo workspace
├── crates/
│   ├── rust-pic/           # 計算コア (main.rs, lxcat.rs, surface.rs)
│   └── server/             # Axum サーバー (main/params/state/jobs/routes/diagnostics)
├── web/                    # React フロント (Vite)
├── xsec/                   # LXCat 断面積データ（コンテナ内 /data/lxcat にマウント）
├── workspaces/             # ジョブ作業領域 (実行時生成)
├── scripts/                # Linux build/run/dev helper scripts
├── deploy/                 # systemd unit example
└── Dockerfile / docker-compose.yml
```

## 必要環境

| ツール | バージョン |
|--------|-----------|
| Rust + Cargo | ≥ 1.75 |
| Node.js | ≥ 20 |
| npm | ≥ 9 |
| （Docker 利用時）Docker | Compose v2 対応版 |

Ubuntu/Debian では、Rust は rustup、Node.js は NodeSource などの Node.js 20+ パッケージで導入してください。Docker だけで実行する場合は Rust/Node をホストへ入れる必要はありません。

## 開発手順

### 推奨スクリプト

```bash
chmod +x scripts/*.sh
./scripts/build.sh  # release バイナリと web/dist を作成
./scripts/run.sh    # release サーバーを 0.0.0.0:8090 で起動
./scripts/dev.sh    # Axum と Vite 開発サーバーを同時起動
```

`scripts/run.sh` は `BIND_ADDR`、`RUST_PIC_BIN`、`WORKSPACES_DIR`、`WEB_DIST`、`LXCAT_DIR` を環境変数で上書きできます。

### 1. バックエンド（Axum サーバー）

```bash
# 計算バイナリとサーバーをビルド
cargo build --release -p rust-pic -p server

# サーバーを起動（既定は 0.0.0.0:8090）
BIND_ADDR=0.0.0.0:8090 RUST_PIC_BIN=target/release/rust-pic cargo run -p server
```

> **ポートに関する注意**: 待受ポートの既定は 8090 です。別のポートにしたい場合は `BIND_ADDR` で変更してください。
> ```bash
> BIND_ADDR=127.0.0.1:9000 cargo run -p server
> ```

### 2. フロント（開発サーバー）

```bash
cd web
npm ci
npm run dev   # http://localhost:5173
```

開発サーバーは `/api` へのリクエストを Axum（既定 `http://127.0.0.1:8090`）にプロキシします。プロキシ先は `VITE_API_TARGET` で変更できます。

### 3. 本番フロントのビルド

```bash
cd web
npm run build   # web/dist を生成（サーバーが静的配信）
```

## Docker での起動

```bash
docker compose up --build
# ブラウザで http://localhost:8090 を開く
```

- `./workspaces` がマウントされ、計算結果が永続化されます。
- `./xsec` が `/data/lxcat` としてマウントされ、LXCat 断面積ファイルをブラウザから選択できます。

## systemd での運用例

`deploy/rust-pic-web.service` は `/opt/rust-pic` へ配置した release ビルドを起動する例です。

```bash
sudo useradd --system --home /opt/rust-pic --shell /usr/sbin/nologin rust-pic
sudo install -d -o rust-pic -g rust-pic /opt/rust-pic/workspaces
sudo cp deploy/rust-pic-web.service /etc/systemd/system/rust-pic-web.service
sudo systemctl daemon-reload
sudo systemctl enable --now rust-pic-web
```

## LXCat ガスモデルの使い方

1. LXCat 形式の断面積ファイル（`.txt`）を `xsec/` ディレクトリに配置する
2. ガスモデルで **LXCat (ファイル選択)** を選択すると、`xsec/` 内のファイル一覧がドロップダウンに表示される
3. ファイルを選択して実行する

## 環境変数

| 変数 | 既定 | 説明 |
|------|------|------|
| `BIND_ADDR` | `0.0.0.0:8090` | 待受アドレス |
| `RUST_PIC_BIN` | `target/release/rust-pic` | 計算バイナリのパス |
| `WORKSPACES_DIR` | `./workspaces` | ジョブ作業領域のルート |
| `WEB_DIST` | `./web/dist` | 静的フロントの配信元 |
| `LXCAT_DIR` | `/data/lxcat` | LXCat 断面積ファイルのディレクトリ |
| `MAX_CONCURRENT` | `4` | 同時実行ジョブ数の上限 |
| `MAX_JOBS` | `50` | 保持ジョブ数の上限（超過分は古い完了ジョブから削除） |

## API エンドポイント

| メソッド・パス | 説明 |
|---|---|
| `POST /api/jobs` | ジョブ投入。`{ params, mode, label?, threads? }` → `{ jobId }` |
| `GET /api/jobs` | ジョブ一覧（status・条件付き、作成時刻降順） |
| `GET /api/jobs/{id}` | ジョブ詳細 |
| `POST /api/jobs/{id}/stop` | 実行中ジョブの停止 |
| `DELETE /api/jobs/{id}` | ジョブ停止＋作業領域削除 |
| `GET /api/jobs/{id}/logs` | ログの SSE ストリーム（`log` / `finished` イベント） |
| `GET /api/jobs/{id}/results` | 結果ファイル名一覧 |
| `GET /api/jobs/{id}/diag/{name}` | 診断ファイルをパースして JSON 返却 |
| `GET /api/lxcat-files` | LXCat ファイル名一覧（`LXCAT_DIR` 配下） |
| `GET /api/health` | ヘルスチェック |

`mode` は `init` / `run` / `measure` / `init_measure` のいずれか。

## 主な計算条件

| 項目 | 既定 | 説明 |
|------|------|------|
| 電圧モード | `rf` | RF（正弦波）/ DC / 矩形波 / sin×sin / square×sin / カスタム波形 |
| RF 振幅 | `250` V | 駆動電圧振幅 |
| 駆動周波数 | `13.56e6` Hz | |
| ガスモデル | `argon-pic` | Argon (PIC) / Air (Go2010) / LXCat（ファイル選択） |
| ガス圧 | `10` Pa | |
| ガス温度 | `350` K | |
| 電極間距離 | `0.001` m | |
| グリッド点数 | `200` | 空間グリッド |
| 周期あたりステップ数 | `4000` | |
| スーパー粒子の重み | `7e4` | |

二次電子放出・Fowler-Nordheim 電界放出・電子反射・イオンサブサイクリング・カスタム波形（CSV アップロード）に対応します。

## 診断出力 (`result/1d/`)

| ファイル | 内容 |
|---------|------|
| `info.txt` | 計算条件のサマリー |
| `density.dat` | 電子・イオン数密度 |
| `field.dat` | 電場 |
| `eepf.dat` | 電子エネルギー確率分布関数 (EEPF) |
| `efed.dat` / `ifed.dat` | 電子・イオンのフラックスエネルギー分布 (FED) |
| `iadf.dat` / `i2adf_*.dat` | イオン角度分布（1D / 2D） |
| `cs.dat` | 衝突断面積 |
| `conv.dat` | 収束履歴 |
| `*_xt.dat` | 時空間 (XT) 2次元マップ（電位・電界・密度・電流・パワー吸収・電離レート・平均エネルギー） |

## Linux Web 版の設計

このリポジトリは Web 版のみを含みます。サーバー構成と API 設計の詳細は `docs/web-migration-spec.md` を参照してください。
