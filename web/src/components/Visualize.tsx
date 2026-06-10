import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { DIAGNOSTIC_CATEGORIES, DIAG_BY_FILE } from "../diagnostics";
import { api } from "../lib/api";
import { LINE_PALETTES, HEATMAP_SCALES, type PlotOptions } from "../lib/plot";
import type { Diagnostic } from "../types";
import { ParticleAnim } from "./ParticleAnim";
import { Plot } from "./Plot";

interface Props {
  jobId: string | null;
  refreshKey: number;
}

export function Visualize({ jobId, refreshKey }: Props) {
  const [results, setResults] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [diag, setDiag] = useState<Diagnostic | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [paletteKey, setPaletteKey] = useState("default");
  const [colorscale, setColorscale] = useState("");
  const [reverse, setReverse] = useState(false);

  // 結果ファイル一覧を取得
  useEffect(() => {
    if (!jobId) {
      setResults([]);
      setSelected(null);
      return;
    }
    api
      .listResults(jobId)
      .then((names) => {
        setResults(names);
        setSelected((cur) => {
          if (cur && names.includes(cur)) return cur;
          // 既定の選択: density.dat → 最初の既知ファイル
          return names.includes("density.dat")
            ? "density.dat"
            : names.find((n) => DIAG_BY_FILE[n]) ?? null;
        });
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [jobId, refreshKey]);

  // 選択された診断を取得
  useEffect(() => {
    if (!jobId || !selected) {
      setDiag(null);
      return;
    }
    let cancelled = false;
    setDiag(null); // 切替時に旧データで再描画しないようクリア
    api
      .readDiagnostic(jobId, selected)
      .then((d) => !cancelled && setDiag(d))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [jobId, selected, refreshKey]);

  const spec = selected ? DIAG_BY_FILE[selected] : undefined;

  // spec と diag の種別が整合するときだけプロットする（不整合の一瞬を描かない）
  const plotReady =
    !!spec &&
    !!diag &&
    ((spec.type === "line" && diag.kind === "columns") ||
      (spec.type === "heatmap" && diag.kind === "matrix"));
  const animReady = !!spec && !!diag && spec.type === "anim" && diag.kind === "anim";

  const opts: PlotOptions = useMemo(
    () => ({
      paletteKey,
      heatmapColorscale: colorscale || undefined,
      reverseScale: reverse,
    }),
    [paletteKey, colorscale, reverse],
  );

  if (!jobId) {
    return (
      <div className="panel">
        <div className="empty-state">
          <div className="glyph">◌</div>
          <div className="msg">計算を実行すると結果がここに表示されます</div>
        </div>
      </div>
    );
  }

  return (
    <div className="viz-layout">
      <aside className="panel results-nav">
        <div className="panel-head">
          <h3>診断</h3>
        </div>
        <div className="results-list">
          {DIAGNOSTIC_CATEGORIES.map((cat) => (
            <div className="diag-cat" key={cat.title}>
              <div className="cat-title">{cat.title}</div>
              {cat.items.map((item) => {
                const available = results.includes(item.file);
                return (
                  <button
                    key={item.file}
                    className={`diag-item ${selected === item.file ? "active" : ""}`}
                    disabled={!available}
                    onClick={() => setSelected(item.file)}
                  >
                    {item.title}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </aside>

      <motion.section
        className="panel"
        key={selected ?? "none"}
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3 }}
      >
        <div className="panel-head">
          <h3>{spec?.title ?? "診断ビュー"}</h3>
          <span className="tag">{selected ?? ""}</span>
        </div>
        <div className="panel-body">
          {error && <div className="notice error">{error}</div>}

          {plotReady && (
            <div className="plot-toolbar">
              {spec.type === "line" && (
                <label className="ctrl">
                  配色
                  <select value={paletteKey} onChange={(e) => setPaletteKey(e.target.value)}>
                    {Object.entries(LINE_PALETTES).map(([k, v]) => (
                      <option key={k} value={k}>
                        {v.label}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {spec.type === "heatmap" && (
                <>
                  <label className="ctrl">
                    カラースケール
                    <select value={colorscale} onChange={(e) => setColorscale(e.target.value)}>
                      <option value="">既定 ({spec.colorscale ?? "Viridis"})</option>
                      {HEATMAP_SCALES.map((s) => (
                        <option key={s} value={s}>
                          {s}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="ctrl">
                    <input
                      type="checkbox"
                      checked={reverse}
                      onChange={(e) => setReverse(e.target.checked)}
                    />
                    反転
                  </label>
                </>
              )}
            </div>
          )}

          {!spec && <div className="empty-state"><div className="msg">診断を選択してください</div></div>}
          {spec && !diag && <div className="empty-state"><div className="msg">読み込み中…</div></div>}
          {spec && diag && diag.kind === "text" && <pre className="report-text">{diag.text}</pre>}
          {animReady && diag.kind === "anim" && <ParticleAnim anim={diag.anim} />}
          {plotReady && <Plot spec={spec!} data={diag!} opts={opts} />}
        </div>
      </motion.section>
    </div>
  );
}
