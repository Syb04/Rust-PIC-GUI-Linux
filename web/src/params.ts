import type { SimParams } from "./types";

// フォームは文字列で値を保持し、実行時に SimParams へ変換する
// (指数表記 1e-15 などをそのまま編集できるようにするため)。
export type FormValues = Record<string, string>;

export type FieldKind = "number" | "int" | "bool" | "select" | "optnumber" | "text";

export interface FieldDef {
  key: string;
  label: string;
  unit?: string;
  kind: FieldKind;
  options?: { value: string; label: string }[];
  help?: string;
}

export interface FieldGroup {
  title: string;
  fields: FieldDef[];
}

// 計算条件の入力グループ定義
export const PARAM_GROUPS: FieldGroup[] = [
  {
    title: "放電条件",
    fields: [
      {
        key: "voltageMode",
        label: "電圧モード",
        kind: "select",
        options: [
          { value: "rf", label: "正弦波 (RF)" },
          { value: "sin-sin", label: "正弦波×正弦波 (sin×sin)" },
          { value: "square", label: "矩形波" },
          { value: "square-sin", label: "矩形波×正弦波 (square×sin)" },
          { value: "dc", label: "DC" },
          { value: "custom", label: "カスタム波形 (CSV)" },
        ],
      },
      { key: "rfAmplitudeV", label: "振幅", unit: "V", kind: "number" },
      {
        key: "poweredDcV",
        label: "印加極 DC 電圧",
        unit: "V",
        kind: "number",
        help: "RF/矩形波モードでは振動の中心電位オフセット、DC モードでは印加電圧",
      },
      { key: "groundedDcV", label: "接地極 DC 電圧", unit: "V", kind: "number" },
      {
        key: "squareDuty",
        label: "デューティ比",
        unit: "%",
        kind: "number",
        help: "矩形波系のデューティ比 (1〜99%、デフォルト 50%)",
      },
      {
        key: "squareRise",
        label: "立ち上がり時間",
        unit: "%",
        kind: "number",
        help: "立ち上がり/立ち下がり時間（周期の割合 0〜49%）。0 = 理想矩形波",
      },
      {
        key: "rf2AmplitudeV",
        label: "第2周波数 振幅",
        unit: "V",
        kind: "number",
        help: "第2周波数成分の振幅 V2（0 = 無効）",
      },
      {
        key: "rf2FrequencyHz",
        label: "第2周波数",
        unit: "Hz",
        kind: "number",
        help: "第2正弦波の周波数 f2（例: 27.12e6）",
      },
    ],
  },
  {
    title: "ガス・ジオメトリ",
    fields: [
      {
        key: "gasModel",
        label: "ガスモデル",
        kind: "select",
        options: [
          { value: "argon-pic", label: "Argon (PIC)" },
          { value: "go2010-air", label: "Air (Go2010 Townsend)" },
          { value: "lxcat", label: "LXCat (ファイル選択)" },
        ],
      },
      { key: "gapM", label: "電極間距離", unit: "m", kind: "number" },
      { key: "pressurePa", label: "ガス圧", unit: "Pa", kind: "number" },
      { key: "temperatureK", label: "ガス温度", unit: "K", kind: "number" },
      { key: "electrodeAreaM2", label: "電極面積", unit: "m²", kind: "number" },
    ],
  },
  {
    title: "LXCat (ガスモデル = lxcat 時)",
    fields: [
      {
        key: "lxcatPath",
        label: "LXCat ファイル",
        kind: "text",
      },
      { key: "gasMassKg", label: "ガス質量 (0=自動)", unit: "kg", kind: "number" },
      { key: "ionMassKg", label: "イオン質量 (0=ガス質量)", unit: "kg", kind: "number" },
      { key: "ionHsSigmaIsoM2", label: "イオン等方HS断面積", unit: "m²", kind: "number" },
      { key: "ionHsSigmaBackM2", label: "イオン後方HS断面積", unit: "m²", kind: "number" },
    ],
  },
  {
    title: "数値計算",
    fields: [
      { key: "gridPoints", label: "グリッド点数", kind: "int" },
      { key: "stepsPerPeriod", label: "周期あたりステップ数", kind: "int" },
      { key: "frequencyHz", label: "駆動周波数", unit: "Hz", kind: "number" },
      { key: "ionSubcycling", label: "イオンサブサイクル", kind: "int" },
      { key: "xtBinSteps", label: "XT ビンステップ", kind: "int" },
      {
        key: "nAdf",
        label: "IADF ビン数",
        kind: "int",
        help: "0°–90° の分割数。デフォルト 90 (1°/bin)",
      },
      {
        key: "n2adf",
        label: "2D IADF ビン数",
        kind: "int",
        help: "-90°–+90° の分割数（各軸）。デフォルト 180",
      },
      { key: "dtS", label: "時間刻み (任意)", unit: "s", kind: "optnumber", help: "空欄で周波数から自動決定" },
      { key: "totalTimeS", label: "総時間 (任意)", unit: "s", kind: "optnumber", help: "空欄で 1/周波数" },
    ],
  },
  {
    title: "粒子",
    fields: [
      { key: "weight", label: "スーパー粒子の重み", kind: "number" },
      { key: "initialParticles", label: "初期粒子数", kind: "int" },
      { key: "maxParticles", label: "最大粒子数", kind: "int" },
    ],
  },
  {
    title: "表面・電子放出",
    fields: [
      { key: "electronReflectionProbability", label: "電子反射確率", kind: "number", help: "0–1" },
      { key: "secondaryYield", label: "二次電子放出係数 γ", kind: "number" },
      { key: "secondaryEnergyEv", label: "二次電子エネルギー", unit: "eV", kind: "number" },
      { key: "fnEmissionEnabled", label: "電界放出 (FN)", kind: "bool" },
      { key: "fnWorkFunctionEv", label: "仕事関数", unit: "eV", kind: "number" },
      { key: "fnFieldEnhancement", label: "電界増倍係数 β", kind: "number" },
      { key: "fnEmissionAreaFactor", label: "放出面積係数", kind: "number" },
      { key: "go2010IonEnhancedFn", label: "イオン誘起電界放出 (Go2010)", kind: "bool" },
      { key: "go2010K", label: "Go2010 係数 K", kind: "number" },
    ],
  },
];

// 実行設定（SimParams 外）のキー
export const RUN_KEYS = ["cycles", "mode", "threads", "label"] as const;

// 既定値 (Rust-PIC の物理デフォルト + 表面の標準設定)
export const DEFAULT_FORM: FormValues = {
  // 実行設定 (SimParams 外)
  cycles: "500",
  mode: "init_measure",
  threads: "",
  label: "",
  // 放電条件
  voltageMode: "rf",
  rfAmplitudeV: "250.0",
  poweredDcV: "0.0",
  groundedDcV: "0.0",
  squareDuty: "50",
  squareRise: "0",
  rf2AmplitudeV: "0",
  rf2FrequencyHz: "27.12e6",
  // ガス・ジオメトリ
  gasModel: "argon-pic",
  gapM: "0.001",
  pressurePa: "10.0",
  temperatureK: "350.0",
  electrodeAreaM2: "1.0e-4",
  // 数値計算
  gridPoints: "200",
  stepsPerPeriod: "4000",
  frequencyHz: "13.56e6",
  ionSubcycling: "20",
  xtBinSteps: "20",
  nAdf: "90",
  n2adf: "180",
  dtS: "",
  totalTimeS: "",
  // 粒子
  weight: "7.0e4",
  initialParticles: "1000",
  maxParticles: "200000",
  // 表面
  electronReflectionProbability: "0.2",
  secondaryYield: "0.01",
  secondaryEnergyEv: "2.0",
  fnEmissionEnabled: "true",
  fnWorkFunctionEv: "4.0",
  fnFieldEnhancement: "100",
  fnEmissionAreaFactor: "1.0",
  go2010IonEnhancedFn: "false",
  go2010K: "1.0e7",
  // LXCat
  lxcatPath: "",
  gasMassKg: "0",
  ionMassKg: "0",
  ionHsSigmaIsoM2: "1.0e-18",
  ionHsSigmaBackM2: "1.0e-18",
};

// ---- 変換 / 検証 -------------------------------------------------

function num(form: FormValues, key: string, label: string): number {
  const v = Number(form[key]);
  if (!Number.isFinite(v)) throw new Error(`${label} は数値で入力してください`);
  return v;
}

function positive(v: number, label: string): number {
  if (!(v > 0)) throw new Error(`${label} は正の数で入力してください`);
  return v;
}

function nonneg(v: number, label: string): number {
  if (!(v >= 0)) throw new Error(`${label} は 0 以上で入力してください`);
  return v;
}

function intval(form: FormValues, key: string, label: string, min: number): number {
  const v = num(form, key, label);
  if (!Number.isInteger(v) || v < min)
    throw new Error(`${label} は ${min} 以上の整数で入力してください`);
  return v;
}

function optnum(form: FormValues, key: string, label: string): number | null {
  const raw = (form[key] ?? "").trim();
  if (raw === "") return null;
  return positive(num(form, key, label), label);
}

/** フォーム値を検証して SimParams に変換する。失敗時は Error を投げる。 */
export function buildParams(form: FormValues): SimParams {
  if (form.gasModel === "lxcat" && !form.lxcatPath?.trim()) {
    throw new Error("LXCat ガスモデルではファイルを選択してください");
  }
  const initial = intval(form, "initialParticles", "初期粒子数", 1);
  const reflect = num(form, "electronReflectionProbability", "電子反射確率");
  if (reflect < 0 || reflect > 1)
    throw new Error("電子反射確率は 0–1 の範囲で入力してください");

  return {
    cycles: intval(form, "cycles", "サイクル数", 0),
    voltageMode: (["rf", "dc", "square", "sin-sin", "square-sin", "custom"].includes(form.voltageMode)
      ? form.voltageMode
      : "rf") as SimParams["voltageMode"],
    rfAmplitudeV: num(form, "rfAmplitudeV", "振幅"),
    poweredDcV: num(form, "poweredDcV", "印加極 DC 電圧"),
    groundedDcV: num(form, "groundedDcV", "接地極 DC 電圧"),
    squareDuty: (() => {
      const v = num(form, "squareDuty", "デューティ比");
      if (v <= 0 || v >= 100) throw new Error("デューティ比は 1〜99% の範囲で入力してください");
      return v;
    })(),
    squareRise: (() => {
      const v = num(form, "squareRise", "立ち上がり時間");
      if (v < 0 || v >= 50) throw new Error("立ち上がり時間は 0〜49% の範囲で入力してください");
      return v;
    })(),
    rf2AmplitudeV: num(form, "rf2AmplitudeV", "第2周波数振幅"),
    rf2FrequencyHz: nonneg(num(form, "rf2FrequencyHz", "第2周波数"), "第2周波数"),
    gridPoints: intval(form, "gridPoints", "グリッド点数", 2),
    stepsPerPeriod: intval(form, "stepsPerPeriod", "周期あたりステップ数", 1),
    frequencyHz: positive(num(form, "frequencyHz", "駆動周波数"), "駆動周波数"),
    gasModel:
      form.gasModel === "go2010-air"
        ? "go2010-air"
        : form.gasModel === "lxcat"
          ? "lxcat"
          : "argon-pic",
    gapM: positive(num(form, "gapM", "電極間距離"), "電極間距離"),
    pressurePa: nonneg(num(form, "pressurePa", "ガス圧"), "ガス圧"),
    temperatureK: positive(num(form, "temperatureK", "ガス温度"), "ガス温度"),
    weight: positive(num(form, "weight", "粒子の重み"), "粒子の重み"),
    electrodeAreaM2: positive(num(form, "electrodeAreaM2", "電極面積"), "電極面積"),
    initialParticles: initial,
    maxParticles: intval(form, "maxParticles", "最大粒子数", initial),
    ionSubcycling: intval(form, "ionSubcycling", "イオンサブサイクル", 1),
    xtBinSteps: intval(form, "xtBinSteps", "XT ビンステップ", 1),
    nAdf: intval(form, "nAdf", "IADF ビン数", 1),
    n2adf: intval(form, "n2adf", "2D IADF ビン数", 1),
    electronReflectionProbability: reflect,
    secondaryYield: nonneg(num(form, "secondaryYield", "二次電子放出係数"), "二次電子放出係数"),
    secondaryEnergyEv: nonneg(num(form, "secondaryEnergyEv", "二次電子エネルギー"), "二次電子エネルギー"),
    fnEmissionEnabled: form.fnEmissionEnabled === "true",
    fnWorkFunctionEv: positive(num(form, "fnWorkFunctionEv", "仕事関数"), "仕事関数"),
    fnFieldEnhancement: nonneg(num(form, "fnFieldEnhancement", "電界増倍係数"), "電界増倍係数"),
    fnEmissionAreaFactor: nonneg(num(form, "fnEmissionAreaFactor", "放出面積係数"), "放出面積係数"),
    go2010IonEnhancedFn: form.go2010IonEnhancedFn === "true",
    go2010K: nonneg(num(form, "go2010K", "Go2010 係数 K"), "Go2010 係数 K"),
    dtS: optnum(form, "dtS", "時間刻み"),
    totalTimeS: optnum(form, "totalTimeS", "総時間"),
    lxcatPath: form.lxcatPath?.trim() ? form.lxcatPath.trim() : null,
    gasMassKg: Math.max(0, Number(form.gasMassKg) || 0),
    ionMassKg: Math.max(0, Number(form.ionMassKg) || 0),
    ionHsSigmaIsoM2: Math.max(0, Number(form.ionHsSigmaIsoM2) || 0),
    ionHsSigmaBackM2: Math.max(0, Number(form.ionHsSigmaBackM2) || 0),
  };
}

/** CSV テキストを波形データ [[時刻µs, 電圧V], ...] にパースする。 */
export function parseWaveformCsv(text: string): [number, number][] {
  const data: [number, number][] = [];
  const lines = text.split(/\r?\n/);
  lines.forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const parts = line.includes(",") ? line.split(",") : line.split(/\s+/);
    if (parts.length < 2) throw new Error(`${i + 1} 行目: 時刻と電圧の2列が必要です`);
    const t = Number(parts[0].trim());
    const v = Number(parts[1].trim());
    if (!Number.isFinite(t) || !Number.isFinite(v))
      throw new Error(`${i + 1} 行目: 数値として解釈できません`);
    data.push([t, v]);
  });
  if (data.length === 0) throw new Error("波形データが空です");
  return data;
}
