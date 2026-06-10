// 計算条件。Rust 側 SimParams (serde camelCase) と 1:1 対応。
export interface SimParams {
  cycles: number;
  voltageMode: "rf" | "dc" | "square" | "sin-sin" | "square-sin" | "custom";
  rfAmplitudeV: number;
  poweredDcV: number;
  groundedDcV: number;
  squareDuty?: number;
  squareRise?: number;
  rf2AmplitudeV?: number;
  rf2FrequencyHz?: number;
  waveformFilePath?: string | null;
  customWaveformData?: [number, number][] | null;
  gridPoints: number;
  stepsPerPeriod: number;
  frequencyHz: number;
  gasModel: "argon-pic" | "go2010-air" | "lxcat";
  lxcatPath?: string | null;
  gasMassKg?: number;
  ionMassKg?: number;
  ionHsSigmaIsoM2?: number;
  ionHsSigmaBackM2?: number;
  gapM: number;
  pressurePa: number;
  temperatureK: number;
  weight: number;
  electrodeAreaM2: number;
  initialParticles: number;
  maxParticles: number;
  ionSubcycling: number;
  xtBinSteps: number;
  nAdf?: number;
  n2adf?: number;
  electronReflectionProbability: number;
  secondaryYield: number;
  secondaryEnergyEv: number;
  fnEmissionEnabled: boolean;
  fnWorkFunctionEv: number;
  fnFieldEnhancement: number;
  fnEmissionAreaFactor: number;
  go2010IonEnhancedFn: boolean;
  go2010K: number;
  dtS?: number | null;
  totalTimeS?: number | null;
}

export type RunMode = "init" | "run" | "measure" | "init_measure";

// read_diagnostic の戻り値 (tagged union)
export type Diagnostic =
  | { kind: "columns"; columns: number[][]; labels?: string[] | null }
  | { kind: "matrix"; matrix: number[][] }
  | { kind: "text"; text: string };

export interface FinishedPayload {
  code: number;
  stopped: boolean;
}

export type JobStatusKind = "queued" | "running" | "done" | "failed" | "stopped";

// GET /api/jobs と /api/jobs/:id のレスポンス。
// サーバー側 JobSummary が meta と status を flatten したフラット構造。
export interface JobSummary {
  id: string;
  createdAt: string;
  label?: string | null;
  params: SimParams;
  status: JobStatusKind;
  code?: number; // status === "done"
  message?: string; // status === "failed"
}

export interface CreateJobRequest {
  params: SimParams;
  mode: RunMode;
  label?: string | null;
  threads?: number | null;
}
