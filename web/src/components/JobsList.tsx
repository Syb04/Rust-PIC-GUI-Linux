import { motion } from "framer-motion";
import type { JobSummary } from "../types";
import { StatusBadge } from "./StatusBadge";
import { jobSummaryLine } from "../lib/diff";

interface Props {
  jobs: JobSummary[];
  selectedIds: string[];
  loading: boolean;
  onToggleSelect: (id: string) => void;
  onView: (id: string) => void;
  onStop: (id: string) => void;
  onDelete: (id: string) => void;
  onCompare: () => void;
  onRefresh: () => void;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("ja-JP", { hour12: false });
}

export function JobsList({
  jobs,
  selectedIds,
  loading,
  onToggleSelect,
  onView,
  onStop,
  onDelete,
  onCompare,
  onRefresh,
}: Props) {
  return (
    <section className="panel">
      <div className="panel-head">
        <h2>ジョブ一覧</h2>
        <span className="tag">{jobs.length} 件</span>
        <div style={{ marginLeft: "auto", display: "flex", gap: 10 }}>
          <button className="btn ghost" onClick={onRefresh} disabled={loading}>
            ⟳ 更新
          </button>
          <button className="btn primary" onClick={onCompare} disabled={selectedIds.length < 2}>
            ◫ 選択を比較 ({selectedIds.length})
          </button>
        </div>
      </div>
      <div className="panel-body">
        {jobs.length === 0 ? (
          <div className="empty-state">
            <div className="glyph">▤</div>
            <div className="msg">ジョブがありません。「計算条件」から計算を開始してください</div>
          </div>
        ) : (
          <div className="jobs-grid">
            {jobs.map((j, i) => {
              const selected = selectedIds.includes(j.id);
              return (
                <motion.div
                  key={j.id}
                  className={`job-card ${selected ? "selected" : ""}`}
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.3, delay: Math.min(i * 0.04, 0.3) }}
                >
                  <div className="job-card-head">
                    <label className="job-check">
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={() => onToggleSelect(j.id)}
                      />
                      <span className="job-label">{j.label?.trim() || j.id.slice(0, 8)}</span>
                    </label>
                    <StatusBadge status={j.status} />
                  </div>
                  <div className="job-summary-line">{jobSummaryLine(j)}</div>
                  <div className="job-meta">{fmtTime(j.createdAt)}</div>
                  {j.status === "failed" && j.message && (
                    <div className="job-error">{j.message}</div>
                  )}
                  <div className="job-actions">
                    <button
                      className="btn ghost"
                      onClick={() => onView(j.id)}
                      disabled={j.status === "queued"}
                    >
                      可視化
                    </button>
                    {j.status === "running" && (
                      <button className="btn danger" onClick={() => onStop(j.id)}>
                        停止
                      </button>
                    )}
                    <button className="btn ghost danger" onClick={() => onDelete(j.id)}>
                      削除
                    </button>
                  </div>
                </motion.div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
