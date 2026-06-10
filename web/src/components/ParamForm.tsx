import { motion } from "framer-motion";
import { useEffect, useState } from "react";
import { PARAM_GROUPS, type FieldDef, type FormValues } from "../params";
import { Field } from "./Field";
import { WaveformUpload } from "./WaveformUpload";

// 実行設定（SimParams 外）のフィールド
const RUN_FIELDS: FieldDef[] = [
  { key: "label", label: "ラベル (任意)", kind: "text", help: "一覧・比較で識別する名前" },
  {
    key: "mode",
    label: "実行モード",
    kind: "select",
    options: [
      { value: "init_measure", label: "初期化 → 測定" },
      { value: "init", label: "初期化のみ" },
      { value: "run", label: "計算のみ" },
      { value: "measure", label: "測定付き計算" },
    ],
  },
  { key: "cycles", label: "サイクル数", kind: "int" },
  {
    key: "threads",
    label: "スレッド数 (任意)",
    kind: "text",
    help: "空欄で自動 (RAYON_NUM_THREADS)",
  },
];

interface Props {
  form: FormValues;
  onChange: (key: string, value: string) => void;
  disabled?: boolean;
  waveform: [number, number][] | null;
  onWaveform: (data: [number, number][] | null, error?: string) => void;
}

export function ParamForm({ form, onChange, disabled, waveform, onWaveform }: Props) {
  const isCustom = form.voltageMode === "custom";
  const isLxcat = form.gasModel === "lxcat";

  const [lxcatFiles, setLxcatFiles] = useState<string[]>([]);
  useEffect(() => {
    if (!isLxcat) return;
    fetch("/api/lxcat-files")
      .then((r) => r.json())
      .then((files: string[]) => setLxcatFiles(files))
      .catch(() => setLxcatFiles([]));
  }, [isLxcat]);

  // LXCat グループはガスモデルが lxcat のときのみ表示
  const groups = PARAM_GROUPS.filter(
    (g) => !g.title.startsWith("LXCat") || isLxcat,
  );

  const cards = [{ title: "実行設定", fields: RUN_FIELDS }, ...groups];

  return (
    <div className="param-grid">
      {cards.map((group, gi) => (
        <motion.section
          key={group.title}
          className="panel field-group"
          initial={{ opacity: 0, y: 14 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: gi * 0.05, ease: [0.16, 1, 0.3, 1] }}
        >
          <div className="panel-body">
            <h3 className="group-title">{group.title}</h3>
            {group.fields.map((def) =>
              def.key === "lxcatPath" ? (
                <div key={def.key} className="field-row">
                  <label className="field-label">{def.label}</label>
                  <select
                    className="field-input"
                    value={form.lxcatPath ?? ""}
                    onChange={(e) => onChange("lxcatPath", e.target.value)}
                    disabled={disabled}
                  >
                    <option value="">-- ファイルを選択 --</option>
                    {lxcatFiles.map((f) => (
                      <option key={f} value={f}>{f}</option>
                    ))}
                  </select>
                </div>
              ) : (
                <Field key={def.key} def={def} form={form} onChange={onChange} disabled={disabled} />
              )
            )}
            {group.title === "放電条件" && isCustom && (
              <WaveformUpload data={waveform} onData={onWaveform} disabled={disabled} />
            )}
          </div>
        </motion.section>
      ))}
    </div>
  );
}
