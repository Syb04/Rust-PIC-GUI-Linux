import { useRef } from "react";
import { parseWaveformCsv } from "../params";

interface Props {
  data: [number, number][] | null;
  onData: (data: [number, number][] | null, error?: string) => void;
  disabled?: boolean;
}

export function WaveformUpload({ data, onData, disabled }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);

  const handleFile = (file: File) => {
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const parsed = parseWaveformCsv(String(reader.result));
        onData(parsed);
      } catch (e) {
        onData(null, e instanceof Error ? e.message : String(e));
      }
    };
    reader.onerror = () => onData(null, "ファイル読み込みに失敗しました");
    reader.readAsText(file);
  };

  return (
    <div className="field">
      <span className="field-label">カスタム波形 (CSV: 時刻[µs], 電圧[V])</span>
      <div
        className="waveform-drop"
        onClick={() => !disabled && inputRef.current?.click()}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          if (disabled) return;
          const f = e.dataTransfer.files?.[0];
          if (f) handleFile(f);
        }}
      >
        {data ? "別のファイルを選択 / ドロップ" : "クリックまたはドロップで CSV を選択"}
      </div>
      <input
        ref={inputRef}
        type="file"
        accept=".csv,.txt,text/csv,text/plain"
        style={{ display: "none" }}
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) handleFile(f);
        }}
      />
      {data && <div className="waveform-info">✓ {data.length} 点を読み込みました</div>}
    </div>
  );
}
