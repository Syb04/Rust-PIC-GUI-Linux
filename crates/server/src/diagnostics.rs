//! 診断結果ファイルのパーサー
//! gui/src-tauri/src/lib.rs の read_columns / Diagnostic / read_diagnostic / list_results を移植

use std::path::Path;

use serde::Serialize;

/// 空白区切りの数値テーブルを行ごとに読む。
/// '#' で始まる行はスキップ、空行もスキップ。
pub fn read_columns(path: &Path) -> Result<Vec<Vec<f64>>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let row: Vec<f64> = line
            .split_whitespace()
            .filter_map(|t| t.parse::<f64>().ok())
            .collect();
        if !row.is_empty() {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// 診断データの種別。serde tag で "kind" フィールドに小文字で出力する。
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Diagnostic {
    /// 列データ (density, eepf, fed, cs など)。columns[列番号][行]
    Columns {
        columns: Vec<Vec<f64>>,
        labels: Option<Vec<String>>,
    },
    /// XT 2D マップ。matrix[空間グリッド][時間ビン]
    Matrix { matrix: Vec<Vec<f64>> },
    /// テキストレポート
    Text { text: String },
}

/// workdir/result/1d/<name> を読み、種別に応じてパースして返す。
pub fn read_diagnostic(workdir: &Path, name: &str) -> Result<Diagnostic, String> {
    let path = workdir.join("result/1d").join(name);
    if !path.exists() {
        return Err(format!("ファイルがありません: {}", path.display()));
    }

    if name == "info.txt" {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return Ok(Diagnostic::Text { text });
    }

    let rows = read_columns(&path)?;

    if name.contains("_xt") || name.starts_with("i2adf") {
        // 行=空間グリッド、列=時間ビン の 2D マップ
        Ok(Diagnostic::Matrix { matrix: rows })
    } else {
        // 行→列へ転置（欠損は NAN で埋める）
        let ncol = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut columns = vec![Vec::with_capacity(rows.len()); ncol];
        for r in &rows {
            for c in 0..ncol {
                columns[c].push(r.get(c).copied().unwrap_or(f64::NAN));
            }
        }

        // 先頭が '#' のヘッダ行があれば列ラベルを抽出（タブ区切り、2個以上のとき有効）
        let labels = std::fs::read_to_string(&path).ok().and_then(|t| {
            t.lines().next().and_then(|line| {
                let line = line.trim_start();
                if line.starts_with('#') {
                    let parts: Vec<String> = line
                        .trim_start_matches('#')
                        .split("	")
                        .map(|s: &str| s.trim().to_string())
                        .filter(|s: &String| !s.is_empty())
                        .collect();
                    if parts.len() >= 2 {
                        Some(parts)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        });

        Ok(Diagnostic::Columns { columns, labels })
    }
}

/// workdir/result/1d 内のファイル名一覧をソートして返す。
/// ディレクトリが無ければ空 Vec。
pub fn list_results(workdir: &Path) -> Vec<String> {
    let dir = workdir.join("result/1d");
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if let Some(n) = e.file_name().to_str() {
                names.push(n.to_string());
            }
        }
    }
    names.sort();
    names
}
