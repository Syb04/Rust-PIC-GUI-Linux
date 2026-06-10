//! LXCat 形式の電子衝突断面積パーサ
//!
//! www.lxcat.net からダウンロードしたテキストを解析する。各衝突過程は
//! 「種別キーワード行 / 標的種行 / (質量比 or しきい値) 行 / メタ行 / `-----`
//! で囲まれたエネルギー・断面積テーブル」というブロック構造を持つ。

use std::fs;
use std::path::Path;

/// 衝突過程の種別
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessKind {
    Elastic,    // 弾性運動量移行
    Effective,  // 実効運動量移行 (弾性 + 全非弾性)
    Excitation, // 励起
    Ionization, // 電離
    Attachment, // 電子付着
}

impl ProcessKind {
    fn from_keyword(s: &str) -> Option<ProcessKind> {
        match s {
            "ELASTIC" => Some(ProcessKind::Elastic),
            "EFFECTIVE" => Some(ProcessKind::Effective),
            "EXCITATION" => Some(ProcessKind::Excitation),
            "IONIZATION" => Some(ProcessKind::Ionization),
            "ATTACHMENT" => Some(ProcessKind::Attachment),
            _ => None,
        }
    }
}

/// 1 つの衝突過程
#[derive(Clone, Debug)]
pub struct LxcatProcess {
    pub kind: ProcessKind,
    pub target: String,
    pub product: Option<String>,
    /// 励起・電離のしきい値 (エネルギー損失) [eV]。弾性・付着は 0。
    pub threshold_ev: f64,
    /// 弾性・effective の 電子質量/標的質量 比。その他は 0。
    pub mass_ratio: f64,
    /// (エネルギー [eV], 断面積 [m^2]) の昇順テーブル
    pub table: Vec<(f64, f64)>,
}

fn is_dashes(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 5 && t.chars().all(|c| c == '-')
}

fn parse_leading_number(line: &str) -> Option<f64> {
    line.trim().split_whitespace().next()?.parse::<f64>().ok()
}

fn parse_pair(line: &str) -> Option<(f64, f64)> {
    let mut it = line.split_whitespace();
    let e = it.next()?.parse::<f64>().ok()?;
    let s = it.next()?.parse::<f64>().ok()?;
    Some((e, s))
}

/// 標的行を "A -> B" / "A <-> B" / "A" に分解する。
fn split_target(line: &str) -> (String, Option<String>) {
    let t = line.trim();
    let split_at = |idx: usize, len: usize| -> (String, Option<String>) {
        let a = t[..idx].trim().to_string();
        let b = t[idx + len..].trim().to_string();
        (a, if b.is_empty() { None } else { Some(b) })
    };
    if let Some(idx) = t.find("<->") {
        split_at(idx, 3)
    } else if let Some(idx) = t.find("->") {
        split_at(idx, 2)
    } else {
        (t.to_string(), None)
    }
}

/// LXCat テキストを解析して衝突過程のリストを返す。
pub fn parse_lxcat(text: &str) -> Result<Vec<LxcatProcess>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut procs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let kind = match ProcessKind::from_keyword(lines[i].trim()) {
            Some(k) => k,
            None => {
                i += 1;
                continue;
            }
        };

        let (target, product) = split_target(lines.get(i + 1).copied().unwrap_or(""));
        let mut j = i + 2;
        let mut threshold = 0.0;
        let mut mass_ratio = 0.0;

        // 3 行目 (数値): 付着以外。弾性/effective は質量比、励起/電離はしきい値。
        if kind != ProcessKind::Attachment {
            if let Some(val) = lines.get(j).and_then(|l| parse_leading_number(l)) {
                match kind {
                    ProcessKind::Elastic | ProcessKind::Effective => mass_ratio = val,
                    _ => threshold = val,
                }
                j += 1;
            }
        }

        // データブロック開始 (最初の dashes) を探す。次ブロックが来たら中断。
        while j < lines.len()
            && !is_dashes(lines[j])
            && ProcessKind::from_keyword(lines[j].trim()).is_none()
        {
            j += 1;
        }

        let mut table = Vec::new();
        if j < lines.len() && is_dashes(lines[j]) {
            j += 1; // 開始 dashes をスキップ
            while j < lines.len() && !is_dashes(lines[j]) {
                if let Some(pair) = parse_pair(lines[j]) {
                    table.push(pair);
                }
                j += 1;
            }
            if j < lines.len() {
                j += 1; // 終端 dashes をスキップ
            }
        }

        if !table.is_empty() {
            table.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            procs.push(LxcatProcess {
                kind,
                target,
                product,
                threshold_ev: threshold,
                mass_ratio,
                table,
            });
        }
        i = j.max(i + 1);
    }

    if procs.is_empty() {
        return Err("LXCat: 断面積ブロックが見つかりませんでした".to_string());
    }
    Ok(procs)
}

/// LXCat ファイルを読み込んで解析する。
pub fn parse_lxcat_file<P: AsRef<Path>>(path: P) -> Result<Vec<LxcatProcess>, String> {
    let text = fs::read_to_string(path.as_ref())
        .map_err(|e| format!("LXCat ファイル読込失敗 {}: {e}", path.as_ref().display()))?;
    parse_lxcat(&text)
}

/// 昇順テーブルからエネルギー e [eV] の断面積を線形補間する。
/// 範囲外は端の値でクランプする。
pub fn interpolate(table: &[(f64, f64)], e: f64) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    if e <= table[0].0 {
        return table[0].1;
    }
    let n = table.len();
    if e >= table[n - 1].0 {
        return table[n - 1].1;
    }
    let mut lo = 0;
    let mut hi = n - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if table[mid].0 <= e {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (e0, s0) = table[lo];
    let (e1, s1) = table[hi];
    if e1 == e0 {
        return s0;
    }
    s0 + (s1 - s0) * (e - e0) / (e1 - e0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
ELASTIC
He
 1.360000e-4
SPECIES: e / He
PROCESS: E + He -> E + He, Elastic
COLUMNS: Energy (eV) | Cross section (m2)
-----------------------------
 0.000000e+0\t4.903500e-20
 1.000000e+0\t5.000000e-20
-----------------------------

EXCITATION
He <-> He*
 1.98e+1
COMMENT: test
-----------------------------
 1.980000e+1\t0.000000e+0
 2.500000e+1\t1.000000e-22
-----------------------------

IONIZATION
He -> He^+
 2.459000e+1
-----------------------------
 2.459000e+1\t0.000000e+0
 1.000000e+2\t3.000000e-21
-----------------------------
";

    #[test]
    fn parses_three_blocks() {
        let p = parse_lxcat(SAMPLE).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].kind, ProcessKind::Elastic);
        assert_eq!(p[0].target, "He");
        assert!((p[0].mass_ratio - 1.36e-4).abs() < 1e-10);
        assert_eq!(p[1].kind, ProcessKind::Excitation);
        assert_eq!(p[1].product.as_deref(), Some("He*"));
        assert!((p[1].threshold_ev - 19.8).abs() < 1e-6);
        assert_eq!(p[2].kind, ProcessKind::Ionization);
        assert!((p[2].threshold_ev - 24.59).abs() < 1e-6);
    }

    #[test]
    fn interpolates_linear() {
        let p = parse_lxcat(SAMPLE).unwrap();
        let s = interpolate(&p[0].table, 0.5);
        assert!((s - 4.95175e-20).abs() < 1e-24);
        // 範囲外クランプ
        assert_eq!(interpolate(&p[0].table, -1.0), 4.9035e-20);
        assert_eq!(interpolate(&p[0].table, 1000.0), 5.0e-20);
    }

    #[test]
    #[ignore = "実ファイル依存 (xsec/)。cargo test -- --ignored で実行"]
    fn parses_real_files() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/xsec/");
        let count = |path: &str, kind: ProcessKind| {
            parse_lxcat_file(path)
                .unwrap()
                .iter()
                .filter(|p| p.kind == kind)
                .count()
        };
        let he = format!("{dir}He Cross section.txt");
        assert_eq!(count(&he, ProcessKind::Elastic), 1);
        assert_eq!(count(&he, ProcessKind::Excitation), 2);
        assert_eq!(count(&he, ProcessKind::Ionization), 1);

        let o2 = format!("{dir}O2 Cross section.txt");
        assert_eq!(count(&o2, ProcessKind::Excitation), 11);
        assert_eq!(count(&o2, ProcessKind::Attachment), 2);

        let cf4 = format!("{dir}CF4 Cross section.txt");
        assert_eq!(count(&cf4, ProcessKind::Excitation), 4);
        assert_eq!(count(&cf4, ProcessKind::Attachment), 1);
    }
}
