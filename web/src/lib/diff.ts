import type { JobSummary, SimParams } from "../types";

// 比較ラベルの自動生成で対象とする主要パラメータ（表示名付き）
const LABEL_KEYS: { key: keyof SimParams; name: string; unit?: string }[] = [
  { key: "voltageMode", name: "波形" },
  { key: "rfAmplitudeV", name: "振幅", unit: "V" },
  { key: "frequencyHz", name: "f", unit: "Hz" },
  { key: "pressurePa", name: "P", unit: "Pa" },
  { key: "temperatureK", name: "T", unit: "K" },
  { key: "gasModel", name: "ガス" },
  { key: "gapM", name: "gap", unit: "m" },
  { key: "poweredDcV", name: "Vdc", unit: "V" },
  { key: "cycles", name: "cycles" },
  { key: "gridPoints", name: "grid" },
  { key: "stepsPerPeriod", name: "steps" },
];

function fmt(v: unknown): string {
  if (typeof v === "number") {
    // 大きい/小さい値は指数表記
    if (v !== 0 && (Math.abs(v) >= 1e5 || Math.abs(v) < 1e-3)) return v.toExponential(2);
    return String(v);
  }
  return String(v);
}

/**
 * 選択ジョブ群について、識別用のラベルを生成する。
 * - ユーザー指定ラベルがあれば優先。
 * - なければ、ジョブ間で値が異なるパラメータから差分ラベルを作る。
 * - 差分が無ければ作成時刻の短縮を使う。
 */
export function computeJobLabels(jobs: JobSummary[]): Record<string, string> {
  // 値が複数種類あるキーを差分キーとして抽出
  const diffKeys = LABEL_KEYS.filter(({ key }) => {
    const vals = new Set(jobs.map((j) => fmt(j.params[key])));
    return vals.size > 1;
  });

  const out: Record<string, string> = {};
  for (const j of jobs) {
    if (j.label && j.label.trim()) {
      out[j.id] = j.label.trim();
      continue;
    }
    if (diffKeys.length > 0) {
      out[j.id] = diffKeys
        .map(({ key, name, unit }) => `${name}=${fmt(j.params[key])}${unit ?? ""}`)
        .join(" · ");
    } else {
      out[j.id] = j.id.slice(0, 8);
    }
  }
  return out;
}

/** 1 つのジョブの短い条件サマリ（一覧カード用）。 */
export function jobSummaryLine(j: JobSummary): string {
  const p = j.params;
  const gas = { "argon-pic": "Ar", "go2010-air": "Air", lxcat: "LXCat" }[p.gasModel] ?? p.gasModel;
  return `${gas} · ${fmt(p.pressurePa)}Pa · ${fmt(p.rfAmplitudeV)}V · ${fmt(p.frequencyHz)}Hz · ${p.cycles}cyc`;
}
