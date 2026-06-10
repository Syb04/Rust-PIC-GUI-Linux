import Plotly from "plotly.js-dist-min";
import type { Diagnostic } from "../types";
import type { DiagSpec } from "../diagnostics";

// Plasma Instrument テーマ: 計器パネルに馴染ませるため背景は透明。
const PAPER = "rgba(0,0,0,0)";
const PLOT_BG = "rgba(8,12,20,0.35)";
const FONT = { color: "#aeb9cc", family: "'IBM Plex Mono', monospace", size: 12 };
const GRID = "rgba(86,104,135,0.18)";
const ZERO = "rgba(86,104,135,0.4)";

// ライン用カラーパレット（電子=シアン, イオン=アンバー を基調に）
export const LINE_PALETTES: Record<string, { label: string; colors: string[] }> = {
  default: {
    label: "プラズマ",
    colors: ["#36e0d6", "#ff9d4d", "#a78bfa", "#5ad19b", "#ff6b8b", "#ffd166"],
  },
  vivid: {
    label: "ビビッド",
    colors: ["#2196f3", "#f44336", "#4caf50", "#9c27b0", "#ff9800", "#00bcd4"],
  },
  colorblind: {
    label: "色覚多様性対応",
    colors: ["#56B4E9", "#E69F00", "#009E73", "#CC79A7", "#F0E442", "#0072B2"],
  },
  mono: {
    label: "モノクロ",
    colors: ["#e0e6f0", "#b0b8c8", "#808a9c", "#606978", "#404853", "#a0a8b8"],
  },
};

// ヒートマップ用カラースケール
export const HEATMAP_SCALES: string[] = [
  "Viridis",
  "Cividis",
  "Inferno",
  "Plasma",
  "Hot",
  "Jet",
  "RdBu",
  "Portland",
  "Electric",
  "YlGnBu",
  "YlOrRd",
  "Greys",
];

export interface PlotOptions {
  paletteKey?: string;
  heatmapColorscale?: string;
  reverseScale?: boolean;
  zmin?: number | null;
  zmax?: number | null;
  showTitle?: boolean;
}

function baseLayout(spec: DiagSpec, opts: PlotOptions): Record<string, unknown> {
  return {
    title:
      opts.showTitle === false
        ? undefined
        : { text: spec.title, font: { color: "#e6ecf7", size: 15, family: "'Chakra Petch', sans-serif" } },
    paper_bgcolor: PAPER,
    plot_bgcolor: PLOT_BG,
    font: FONT,
    margin: { l: 78, r: 28, t: opts.showTitle === false ? 12 : 48, b: 58 },
    autosize: true,
    showlegend: true,
    legend: { orientation: "h", y: -0.22, font: { size: 11 } },
  };
}

function axis(title: string | undefined, log: boolean | undefined): Record<string, unknown> {
  return {
    title: { text: title ?? "", font: { size: 12 } },
    type: log ? "log" : "linear",
    gridcolor: GRID,
    zerolinecolor: ZERO,
    linecolor: GRID,
    tickfont: { size: 11 },
  };
}

const CONFIG = { responsive: true, displaylogo: false };

/** 仕様とデータから Plotly 図を要素へ描画する。 */
export function plotDiagnostic(
  el: HTMLElement,
  spec: DiagSpec,
  data: Diagnostic,
  opts: PlotOptions = {},
): void {
  if (data.kind === "columns" && spec.type === "line") {
    const cols = data.columns;
    const xCol = spec.xCol ?? 0;
    const x = cols[xCol] ?? [];
    const palette =
      (opts.paletteKey && LINE_PALETTES[opts.paletteKey]?.colors) ||
      LINE_PALETTES.default.colors;
    const labels = data.labels ?? undefined;
    const traceDefs =
      spec.traces ??
      cols
        .map((_, i) => i)
        .filter((i) => i !== xCol)
        .map((i) => ({ col: i, name: labels?.[i] ?? `列 ${i}` }));

    const traces = traceDefs
      .filter((t) => cols[t.col])
      .map((t, idx) => ({
        x,
        y: cols[t.col],
        type: "scatter",
        mode: "lines",
        name: t.name,
        line: { color: palette[idx % palette.length], width: 2 },
      }));

    const layout = {
      ...baseLayout(spec, opts),
      xaxis: axis(spec.xLabel, spec.logX),
      yaxis: axis(spec.yLabel, spec.logY),
    };
    Plotly.react(el, traces, layout, CONFIG);
    return;
  }

  if (data.kind === "matrix" && spec.type === "heatmap") {
    const z = data.matrix;
    const nrows = z.length;
    const ncols = nrows > 0 ? z[0].length : 0;

    let xArr: number[];
    let yArr: number[];
    let xAxisTitle: string;
    let yAxisTitle: string;

    if (spec.xRange && spec.yRange) {
      const [x0, x1] = spec.xRange;
      const [y0, y1] = spec.yRange;
      xArr = Array.from({ length: ncols }, (_, i) =>
        ncols > 1 ? x0 + (i / (ncols - 1)) * (x1 - x0) : (x0 + x1) / 2,
      );
      yArr = Array.from({ length: nrows }, (_, i) =>
        nrows > 1 ? y0 + (i / (nrows - 1)) * (y1 - y0) : (y0 + y1) / 2,
      );
      xAxisTitle = spec.xLabel ?? "";
      yAxisTitle = spec.yLabel ?? "";
    } else {
      xArr = Array.from({ length: ncols }, (_, i) => (ncols > 1 ? i / (ncols - 1) : 0));
      yArr = Array.from({ length: nrows }, (_, i) => (nrows > 1 ? i / (nrows - 1) : 0));
      xAxisTitle = "RF 位相 (規格化)";
      yAxisTitle = "位置 (規格化)";
    }

    const hasZmin = typeof opts.zmin === "number" && Number.isFinite(opts.zmin);
    const hasZmax = typeof opts.zmax === "number" && Number.isFinite(opts.zmax);
    const trace = {
      z,
      x: xArr,
      y: yArr,
      type: "heatmap",
      colorscale: opts.heatmapColorscale || spec.colorscale || "Viridis",
      reversescale: !!opts.reverseScale,
      zauto: !(hasZmin || hasZmax),
      ...(hasZmin ? { zmin: opts.zmin } : {}),
      ...(hasZmax ? { zmax: opts.zmax } : {}),
      colorbar: spec.colorbareExponent
        ? { tickfont: FONT, exponentformat: "e", showexponent: "all" }
        : { tickfont: FONT },
    };
    const layout = {
      ...baseLayout(spec, opts),
      showlegend: false,
      xaxis: { title: { text: xAxisTitle }, gridcolor: GRID, linecolor: GRID, tickfont: { size: 11 } },
      yaxis: { title: { text: yAxisTitle }, gridcolor: GRID, linecolor: GRID, tickfont: { size: 11 } },
    };
    Plotly.react(el, [trace], layout, CONFIG);
    return;
  }

  Plotly.purge(el);
  el.textContent = "このデータは表示できません";
}

export function purgePlot(el: HTMLElement): void {
  Plotly.purge(el);
}

export interface CompareSeries {
  name: string; // ジョブのラベル
  columns: number[][];
  labels?: string[] | null;
}

/** 複数ジョブの line 診断を 1 つの図に重ねて描画する。 */
export function plotCompareLines(
  el: HTMLElement,
  spec: DiagSpec,
  series: CompareSeries[],
  opts: PlotOptions = {},
): void {
  const palette =
    (opts.paletteKey && LINE_PALETTES[opts.paletteKey]?.colors) ||
    LINE_PALETTES.default.colors;
  const dashes = ["solid", "dash", "dot", "dashdot"];

  const traces: Record<string, unknown>[] = [];
  series.forEach((s, si) => {
    const cols = s.columns;
    const xCol = spec.xCol ?? 0;
    const x = cols[xCol] ?? [];
    const labels = s.labels ?? undefined;
    const traceDefs =
      spec.traces ??
      cols
        .map((_, i) => i)
        .filter((i) => i !== xCol)
        .map((i) => ({ col: i, name: labels?.[i] ?? `列 ${i}` }));

    traceDefs
      .filter((t) => cols[t.col])
      .forEach((t, ti) => {
        traces.push({
          x,
          y: cols[t.col],
          type: "scatter",
          mode: "lines",
          name: traceDefs.length > 1 ? `${s.name} · ${t.name}` : s.name,
          line: {
            color: palette[si % palette.length],
            width: 2,
            dash: dashes[ti % dashes.length],
          },
        });
      });
  });

  const layout = {
    ...baseLayout(spec, opts),
    xaxis: axis(spec.xLabel, spec.logX),
    yaxis: axis(spec.yLabel, spec.logY),
  };
  Plotly.react(el, traces, layout, CONFIG);
}
