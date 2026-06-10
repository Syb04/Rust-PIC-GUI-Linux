# 超粒子アニメーション機能 実装仕様

測定実行（measurement mode）の最後の 1 RF 周期について、超粒子（電子・正イオン）の
位相空間 (x–vx) スナップショットを記録し、Web UI で散布図アニメーションとして再生する。

## 全体データフロー

```
rust-pic (最終RF周期で粒子をサンプリング)
  → result/1d/anim.json
  → server: GET /api/jobs/:id/diag/anim.json (新 Diagnostic variant "anim")
  → web: 診断リスト「粒子アニメーション」→ 再生 UI (Plotly scatter)
```

## 1. シミュレータ側 (crates/rust-pic)

### Cargo.toml
- `serde_json = "1.0"` を依存に追加する。

### main.rs
- 測定ループは `for c in 1..=cycle`（~L1466）×`for t in 0..sim_config.steps_per_period`（~L1467）。
  XT 収集は `if measurement`（~L1593）内で行われている。
- **最終周期のみ** (`measurement && c == cycle`) フレームをサンプリングする:
  - フレーム間隔: `let frame_stride = (sim_config.steps_per_period / 200).max(1);`
    `t % frame_stride == 0` のステップで記録（≈200 フレーム/周期）。
  - 粒子間引き: 種ごとに最大 1000 個。`let stride = (vec.len() / 1000).max(1);` で
    `iter().step_by(stride)` により x と vx を収集する。
  - サンプリング位置は XT 収集と同じ場所（粒子移動後）でよい。
- フレーム構造（メモリ上に蓄積し、最後に書き出す）:
  ```json
  {
    "gapM": <電極間隔 m>,
    "framesPerCycle": <フレーム数>,
    "frames": [
      { "t": <周期内規格化時刻 0..1>,
        "ex": [...], "evx": [...],   // 電子の x [m], vx [m/s]
        "ix": [...], "ivx": [...] }  // 正イオンの x [m], vx [m/s]
    ]
  }
  ```
  キーは camelCase。負イオンは対象外（スコープ外）。
- 書き出し: 測定後の保存セクション（density.dat 等を保存している ~L1796 付近）で
  `result/1d/anim.json` に serde_json で書く。他の save_* と同様にエラーは
  `println!` で報告して続行する。
- measurement off のときは一切記録・出力しない。メモリは ~10 MB 程度で許容。

## 2. サーバー側 (crates/server/src/diagnostics.rs)

- `Diagnostic` enum に variant を追加:
  ```rust
  /// 粒子アニメーション (anim.json の中身をそのまま返す)
  Anim { anim: serde_json::Value },
  ```
  serde tag は既存どおり `kind`、小文字 `"anim"` になることを確認する。
- `read_diagnostic` の先頭付近で `name == "anim.json"` を特別扱い:
  ファイルを読んで `serde_json::from_str::<Value>` し `Diagnostic::Anim` で返す。
- `list_results` は result/1d の全ファイルを返すため変更不要。
- routes.rs も変更不要（read_diagnostic 経由で配信される）。

## 3. フロントエンド (web/src)

### types.ts
```ts
export interface AnimFrame { t: number; ex: number[]; evx: number[]; ix: number[]; ivx: number[]; }
export interface AnimData { gapM: number; framesPerCycle: number; frames: AnimFrame[]; }
// Diagnostic union に追加:
| { kind: "anim"; anim: AnimData }
```

### diagnostics.ts
- `DiagSpec.type` に `"anim"` を追加。
- 新カテゴリを追加:
  ```ts
  { title: "粒子アニメーション",
    items: [{ file: "anim.json", title: "超粒子位相空間 (x–vx)", type: "anim" }] }
  ```

### 新コンポーネント components/ParticleAnim.tsx
- props: `{ anim: AnimData }`。
- Plotly で縦 2 段サブプロット（電子は |vx|~1e6 m/s、イオンは ~1e3 m/s と
  スケールが大きく違うため別軸にする）:
  - 上段: 電子 (x, vx) scatter、マーカーサイズ 2–3、色は既存パレットに合わせる
  - 下段: 正イオン (x, vx) scatter
  - x 軸: 0 .. gapM [m]（固定）
  - y 軸: 全フレームから各種の vx min/max を一度計算して固定（フレーム間で軸が動かないこと）
- 再生 UI:
  - 再生/一時停止ボタン、フレームスライダー、速度選択（例: 10/20/40 fps）
  - ループ再生（最終フレームの次は先頭へ）
  - 現在位相 `t/T = 0.xx` を表示
- 実装は `setInterval` または `requestAnimationFrame` でフレーム index を進め、
  Plotly の data を差し替える方式でよい。**既存の `components/Plot.tsx` と
  `lib/plot.ts` の Plotly 利用パターン（import 方法・レイアウトのスタイル）を必ず確認し、
  同じ流儀で実装すること。** アンマウント時に interval を解除する。

### Visualize.tsx
- `plotReady` 相当の判定に `spec.type === "anim" && diag.kind === "anim"` を追加し、
  その場合 `<ParticleAnim anim={diag.anim} />` を描画する。
- 配色ツールバー（パレット/カラースケール）は anim では表示しない。

## 制約・検証

- 既存の診断・出力の挙動を変えない。リファクタリング禁止、最小差分で実装する。
- コメント・UI 文言は既存に合わせて日本語、コード・変数名は英語。
- 検証: `cargo build --release`（ワークスペース全体）と `cd web && npx tsc -b` が通ること。
- コミットは行わない（working tree に変更を残すだけにする）。
