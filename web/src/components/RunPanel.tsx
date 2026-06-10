import { useEffect, useRef } from "react";
import { motion } from "framer-motion";
import type { FormValues } from "../params";
import type { JobStatusKind } from "../types";
import { StatusBadge } from "./StatusBadge";

interface Props {
  form: FormValues;
  status: JobStatusKind | null;
  running: boolean;
  log: string[];
  onStop: () => void;
}

const GAS_LABELS: Record<string, string> = {
  "argon-pic": "Argon (PIC)",
  "go2010-air": "Air (Go2010)",
  lxcat: "LXCat",
};

const MODE_LABELS: Record<string, string> = {
  init_measure: "初期化 → 測定",
  init: "初期化のみ",
  run: "計算のみ",
  measure: "測定付き計算",
};

export function RunPanel({ form, status, running, log, onStop }: Props) {
  const consoleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = consoleRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log]);

  return (
    <div className="run-layout">
      <motion.section
        className="panel run-summary"
        initial={{ opacity: 0, x: -12 }}
        animate={{ opacity: 1, x: 0 }}
        transition={{ duration: 0.35 }}
      >
        <div className="panel-head">
          <h3>実行ステータス</h3>
          {status && <StatusBadge status={status} />}
        </div>
        <div className="panel-body">
          <dl>
            <dt>モード</dt>
            <dd>{MODE_LABELS[form.mode] ?? form.mode}</dd>
            <dt>サイクル</dt>
            <dd>{form.cycles}</dd>
            <dt>ガス</dt>
            <dd>{GAS_LABELS[form.gasModel] ?? form.gasModel}</dd>
            <dt>圧力</dt>
            <dd>{form.pressurePa} Pa</dd>
            <dt>周波数</dt>
            <dd>{form.frequencyHz} Hz</dd>
            <dt>振幅</dt>
            <dd>{form.rfAmplitudeV} V</dd>
            <dt>グリッド</dt>
            <dd>{form.gridPoints}</dd>
          </dl>
          {running && (
            <button className="btn danger" style={{ marginTop: 18, width: "100%" }} onClick={onStop}>
              ■ 停止
            </button>
          )}
        </div>
      </motion.section>

      <section className="panel" style={{ padding: 0 }}>
        <div className="panel-head">
          <h3>シミュレーションログ</h3>
          <span className="tag">stdout / stderr · SSE</span>
        </div>
        <div className="console" ref={consoleRef}>
          {log.length === 0 ? (
            <div className="console-empty">— ログ待機中 —</div>
          ) : (
            log.map((line, i) => {
              const cls = line.startsWith(">>")
                ? "sys"
                : /error|エラー|failed|panic/i.test(line)
                  ? "err"
                  : "";
              return (
                <div key={i} className={`console-line ${cls}`}>
                  {line}
                </div>
              );
            })
          )}
        </div>
      </section>
    </div>
  );
}
