// result/1d 配下の診断ファイルの表示仕様カタログ

export interface LineTrace {
  col: number;
  name: string;
}

export interface DiagSpec {
  file: string;
  title: string;
  type: "line" | "heatmap" | "text" | "anim";
  xLabel?: string;
  yLabel?: string;
  xCol?: number; // line: x 軸の列番号
  traces?: LineTrace[]; // line: y の列
  logX?: boolean;
  logY?: boolean;
  colorscale?: string;
  xRange?: [number, number]; // heatmap: 実際の x 軸範囲
  yRange?: [number, number]; // heatmap: 実際の y 軸範囲
  colorbareExponent?: boolean; // heatmap: カラーバーを指数表記にする
}

export interface DiagCategory {
  title: string;
  items: DiagSpec[];
}

export const DIAGNOSTIC_CATEGORIES: DiagCategory[] = [
  {
    title: "空間プロファイル",
    items: [
      {
        file: "density.dat",
        title: "粒子密度分布",
        type: "line",
        xLabel: "位置 x [m]",
        yLabel: "密度 [m⁻³]",
        xCol: 0,
      },
    ],
  },
  {
    title: "エネルギー分布",
    items: [
      {
        file: "eepf.dat",
        title: "電子エネルギー確率関数 (EEPF)",
        type: "line",
        xLabel: "エネルギー [eV]",
        yLabel: "EEPF [eV⁻³ᐟ²]",
        xCol: 0,
        traces: [{ col: 1, name: "EEPF" }],
        logY: true,
      },
      {
        file: "efed.dat",
        title: "電子フラックスエネルギー分布 (EFED)",
        type: "line",
        xLabel: "エネルギー [eV]",
        yLabel: "確率密度 [eV⁻¹]",
        xCol: 0,
        traces: [
          { col: 1, name: "印加極" },
          { col: 2, name: "接地極" },
        ],
      },
      {
        file: "ifed.dat",
        title: "イオンフラックスエネルギー分布 (IFED)",
        type: "line",
        xLabel: "エネルギー [eV]",
        yLabel: "確率密度 [eV⁻¹]",
        xCol: 0,
        traces: [
          { col: 1, name: "印加極" },
          { col: 2, name: "接地極" },
        ],
      },
      {
        file: "iadf.dat",
        title: "イオン角度分布 (IADF)",
        type: "line",
        xLabel: "角度 [°]",
        yLabel: "確率密度 [deg⁻¹]",
        xCol: 0,
        traces: [
          { col: 1, name: "印加極" },
          { col: 2, name: "接地極" },
        ],
      },
      {
        file: "i2adf_pow.dat",
        title: "2D イオン角度分布 - 印加極",
        type: "heatmap",
        xLabel: "θ₂ = atan2(Vy, Vz) [°]",
        yLabel: "θ₁ = atan2(Vx, Vz) [°]",
        colorscale: "Hot",
        xRange: [-90, 90],
        yRange: [-90, 90],
      },
      {
        file: "i2adf_gnd.dat",
        title: "2D イオン角度分布 - 接地極",
        type: "heatmap",
        xLabel: "θ₂ = atan2(Vy, Vz) [°]",
        yLabel: "θ₁ = atan2(Vx, Vz) [°]",
        colorscale: "Hot",
        xRange: [-90, 90],
        yRange: [-90, 90],
      },
    ],
  },
  {
    title: "断面積・収束",
    items: [
      {
        file: "cs.dat",
        title: "衝突断面積",
        type: "line",
        xLabel: "エネルギー [eV]",
        yLabel: "断面積 [m²]",
        xCol: 0,
        logX: true,
        logY: true,
      },
      {
        file: "conv.dat",
        title: "収束履歴",
        type: "line",
        xLabel: "サイクル",
        yLabel: "粒子数",
        xCol: 0,
        traces: [
          { col: 1, name: "電子 #e" },
          { col: 2, name: "正イオン #i" },
          { col: 3, name: "負イオン #n" },
        ],
        logY: true,
      },
    ],
  },
  {
    title: "時空間 (XT) マップ",
    items: [
      { file: "pot_xt.dat", title: "電位 φ(x,t)", type: "heatmap", colorscale: "RdBu" },
      { file: "efield_xt.dat", title: "電界 E(x,t)", type: "heatmap", colorscale: "RdBu" },
      { file: "ne_xt.dat", title: "電子密度 nₑ(x,t)", type: "heatmap", colorscale: "Viridis", colorbareExponent: true },
      { file: "ni_xt.dat", title: "イオン密度 nᵢ(x,t)", type: "heatmap", colorscale: "Viridis", colorbareExponent: true },
      { file: "nn_xt.dat", title: "負イオン密度 nₙ(x,t)", type: "heatmap", colorscale: "Viridis", colorbareExponent: true },
      { file: "je_xt.dat", title: "電子電流 Jₑ(x,t)", type: "heatmap", colorscale: "RdBu" },
      { file: "ji_xt.dat", title: "イオン電流 Jᵢ(x,t)", type: "heatmap", colorscale: "RdBu" },
      { file: "powere_xt.dat", title: "電子パワー吸収 (x,t)", type: "heatmap", colorscale: "RdBu" },
      { file: "poweri_xt.dat", title: "イオンパワー吸収 (x,t)", type: "heatmap", colorscale: "RdBu" },
      { file: "ioniz_xt.dat", title: "電離レート (x,t)", type: "heatmap", colorscale: "Hot" },
      { file: "meanee_xt.dat", title: "電子平均エネルギー (x,t)", type: "heatmap", colorscale: "Viridis" },
      { file: "meanei_xt.dat", title: "イオン平均エネルギー (x,t)", type: "heatmap", colorscale: "Viridis" },
    ],
  },
  {
    title: "粒子アニメーション",
    items: [{ file: "anim.json", title: "超粒子位相空間 (x-vx)", type: "anim" }],
  },
  {
    title: "レポート",
    items: [{ file: "info.txt", title: "シミュレーションレポート", type: "text" }],
  },
];

// ファイル名 -> 仕様 の逆引き
export const DIAG_BY_FILE: Record<string, DiagSpec> = Object.fromEntries(
  DIAGNOSTIC_CATEGORIES.flatMap((c) => c.items).map((s) => [s.file, s]),
);
