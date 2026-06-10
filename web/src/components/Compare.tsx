import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { DIAGNOSTIC_CATEGORIES, DIAG_BY_FILE } from "../diagnostics";
import { api } from "../lib/api";
import { LINE_PALETTES, HEATMAP_SCALES, type CompareSeries, type PlotOptions } from "../lib/plot";
import { computeJobLabels } from "../lib/diff";
import type { Diagnostic, JobSummary } from "../types";
import { Plot } from "./Plot";
import { ComparePlot } from "./ComparePlot";

interface Props {
  jobs: JobSummary[]; // 比較対象ジョブ（2 件以上）
}

export function Compare({ jobs }: Props) {
  const [resultsByJob, setResultsByJob] = useState<Record<string, string[]>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [dataByJob, setDataByJob] = useState<Record<string, Diagnostic>>({});
  const [error, setError] = useState<string | null>(null);
  const [paletteKey, setPaletteKey] = useState("default");
  const [colorscale, setColorscale] = useState("");

  const labels = useMemo(() => computeJobLabels(jobs), [jobs]);
  const jobIds = jobs.map((j) => j.id).join(",");

  // 各ジョブの結果一覧を取得
  useEffect(() => {
    let cancelled = false;
    Promise.all(
      jobs.map((j) =>
        api
          .listResults(j.id)
          .then((names) => [j.id, names] as const)
          .catch(() => [j.id, [] as string[]] as const),
      ),
    ).then((pairs) => {
      if (cancelled) return;
      const map: Record<string, string[]> = {};
      pairs.forEach(([id, names]) => (map[id] = names));
      setResultsByJob(map);
    });
    return () => {
      cancelled = true;
    };
  }, [jobIds]);

  // 比較可能な診断（line/heatmap のみ、いずれかのジョブに存在するもの）
  const availableSpecs = useMemo(() => {
    const union = new Set<string>();
    Object.values(resultsByJob).forEach((names) => names.forEach((n) => union.add(n)));
    return DIAGNOSTIC_CATEGORIES.flatMap((c) => c.items).filter(
      (s) => s.type !== "text" && union.has(s.file),
    );
  }, [resultsByJob]);

  // 既定選択
  useEffect(() => {
    if (selected && availableSpecs.some((s) => s.file === selected)) return;
    const pref =
      availableSpecs.find((s) => s.file === "eepf.dat") ??
      availableSpecs.find((s) => s.type === "line") ??
      availableSpecs[0];
    setSelected(pref?.file ?? null);
  }, [availableSpecs]);

  // 選択診断を全ジョブ分取得
  useEffect(() => {
    if (!selected) {
      setDataByJob({});
      return;
    }
    let cancelled = false;
    Promise.all(
      jobs
        .filter((j) => resultsByJob[j.id]?.includes(selected))
        .map((j) =>
          api
            .readDiagnostic(j.id, selected)
            .then((d) => [j.id, d] as const)
            .catch(() => null),
        ),
    ).then((pairs) => {
      if (cancelled) return;
      const map: Record<string, Diagnostic> = {};
      pairs.forEach((p) => p && (map[p[0]] = p[1]));
      setDataByJob(map);
      setError(null);
    });
    return () => {
      cancelled = true;
    };
  }, [selected, jobIds, resultsByJob]);

  const spec = selected ? DIAG_BY_FILE[selected] : undefined;

  const lineSeries: CompareSeries[] = useMemo(() => {
    if (!spec || spec.type !== "line") return [];
    return jobs
      .map((j): CompareSeries | null => {
        const d = dataByJob[j.id];
        if (d && d.kind === "columns")
          return { name: labels[j.id] ?? j.id.slice(0, 8), columns: d.columns, labels: d.labels };
        return null;
      })
      .filter((s): s is CompareSeries => s !== null);
  }, [spec, dataByJob, jobs, labels]);

  const opts: PlotOptions = useMemo(
    () => ({ paletteKey, heatmapColorscale: colorscale || undefined }),
    [paletteKey, colorscale],
  );

  return (
    <div className="viz-layout">
      <aside className="panel results-nav">
        <div className="panel-head">
          <h3>比較診断</h3>
          <span className="tag">{jobs.length} ジョブ</span>
        </div>
        <div className="results-list">
          {DIAGNOSTIC_CATEGORIES.map((cat) => {
            const items = cat.items.filter((s) => availableSpecs.some((a) => a.file === s.file));
            if (items.length === 0) return null;
            return (
              <div className="diag-cat" key={cat.title}>
                <div className="cat-title">{cat.title}</div>
                {items.map((item) => (
                  <button
                    key={item.file}
                    className={`diag-item ${selected === item.file ? "active" : ""}`}
                    onClick={() => setSelected(item.file)}
                  >
                    {item.title}
                  </button>
                ))}
              </div>
            );
          })}
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
          <h3>{spec?.title ?? "比較ビュー"}</h3>
          <span className="tag">{selected ?? ""}</span>
        </div>
        <div className="panel-body">
          {error && <div className="notice error">{error}</div>}

          {/* 凡例（ジョブラベル） */}
          <div className="compare-legend">
            {jobs.map((j, i) => (
              <span className="legend-item" key={j.id}>
                <span
                  className="swatch"
                  style={{ background: LINE_PALETTES[paletteKey].colors[i % LINE_PALETTES[paletteKey].colors.length] }}
                />
                {labels[j.id]}
              </span>
            ))}
          </div>

          {spec && (
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
              )}
            </div>
          )}

          {!spec && <div className="empty-state"><div className="msg">比較する診断を選択してください</div></div>}

          {/* line: 重ね描き */}
          {spec && spec.type === "line" && lineSeries.length > 0 && (
            <ComparePlot spec={spec} series={lineSeries} opts={opts} />
          )}

          {/* heatmap: ジョブごとにグリッド表示 */}
          {spec && spec.type === "heatmap" && (
            <div className="heatmap-grid">
              {jobs.map((j) => {
                const d = dataByJob[j.id];
                return (
                  <div className="heatmap-cell" key={j.id}>
                    <div className="heatmap-cell-title">{labels[j.id]}</div>
                    {d && d.kind === "matrix" ? (
                      <Plot spec={spec} data={d} opts={{ ...opts, showTitle: false }} />
                    ) : (
                      <div className="empty-state"><div className="msg">データなし</div></div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </motion.section>
    </div>
  );
}
