import type { JobStatusKind } from "../types";

const LABELS: Record<JobStatusKind, string> = {
  queued: "待機中",
  running: "計算中",
  done: "完了",
  failed: "失敗",
  stopped: "停止",
};

export function StatusBadge({ status }: { status: JobStatusKind }) {
  return (
    <span className={`badge ${status}`}>
      <span className="dot" />
      {LABELS[status]}
    </span>
  );
}
