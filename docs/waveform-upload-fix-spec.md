# カスタム電圧波形 CSV のブラウザ投入 — サーバー側配線修正 仕様書

## 背景

フロント（`web/src/components/WaveformUpload.tsx`）は CSV をパースして `params.customWaveformData: [number, number][]` としてジョブ投入 API に送る。計算コア（`crates/rust-pic`）は `--voltage-mode custom --waveform-file <path>` で波形 CSV を読める。

しかしサーバーは `SimParams.custom_waveform_data`（`crates/server/src/params.rs:80`）を受信するだけで**どこでも使っていない**。`build_args`（params.rs:239-244）は `waveform_file_path` のみ参照し、ブラウザ経由ではこれが None のため、カスタム波形が無視されて計算が走る。

## 修正内容（`crates/server/src/jobs.rs` の `create_job`、192 行目付近）

`create_job` のシグネチャを `mut params: SimParams` に変え、workdir 作成後・`JobMeta::new` の前に以下を追加する:

1. `params.voltage_mode == "custom"` の場合:
   - `params.custom_waveform_data` が `Some` かつ空でなければ:
     - workdir に `waveform.csv` を書き出す（tokio::fs::write）。
     - **書式は計算コアのパーサに合わせること**: `crates/rust-pic/src/main.rs` の `parse_waveform_csv`（926 行目付近）を読み、ヘッダ行・コメント行の扱いを確認した上で、確実に読める形式（基本は 1 行 `時刻,電圧` の数値 2 カラム、`format!("{},{}", t, v)`）で出力する。ルートの `waveform_example.csv` も参考にする。
     - `params.waveform_file_path = Some("waveform.csv".into())` を設定する（サブプロセスは cwd=workdir で起動されるため相対パスでよい）。
   - 波形データも `waveform_file_path` も無い場合は `anyhow::bail!` で明確な日本語エラーを返す（例: 「voltage_mode=custom には波形データが必要です」）。呼び出し元 `routes.rs` のジョブ作成ハンドラがこのエラーを 500 で返しているなら、可能なら 400 (BAD_REQUEST) になるよう調整してよい（大改造は不要）。

2. `JobMeta::new(id, label, params)` には**書き換え後の params**（waveform_file_path 設定済み）が入ること。meta.json 経由のジョブ復元・比較ビューの条件表示に影響しないことを確認する。

## 制約

- 変更は最小限に。`build_args` のロジック・フロントエンドは変更しない（フロントは既に custom 時に波形必須のバリデーション済み）。
- 作業ディレクトリは `C:\works\Rust-PIC-GUI-Linux` のみ。`C:\works\Rust-PIC-GUI` には触れない。
- このツリーは Linux 専用（`process_group` 使用）のため、Windows ホストでの `cargo check` は通らない。コンパイル検証は発注元が Docker で行う。

## 完了報告に含めること

- 変更ファイルと変更概要
- `parse_waveform_csv` の書式仕様（ヘッダ可否）の確認結果と、それに合わせた出力形式の根拠
