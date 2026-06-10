import { useCallback, useEffect, useRef, useState } from "react";
import { ParamForm } from "./components/ParamForm";
import { RunPanel } from "./components/RunPanel";
import { Visualize } from "./components/Visualize";
import { JobsList } from "./components/JobsList";
import { Compare } from "./components/Compare";
import { DEFAULT_FORM, buildParams, type FormValues } from "./params";
import { api, subscribeLogs, type LogSubscription } from "./lib/api";
import type { JobStatusKind, JobSummary, RunMode } from "./types";

const STORAGE_KEY = "rust-pic-web-form";
const MAX_LOG = 2000;

type Tab = "params" | "run" | "jobs" | "visualize";
type ViewMode = "single" | "compare";

function loadForm(): FormValues {
  try {
    const s = localStorage.getItem(STORAGE_KEY);
    if (s) return { ...DEFAULT_FORM, ...JSON.parse(s) };
  } catch {
    /* ignore */
  }
  return { ...DEFAULT_FORM };
}

export default function App() {
  const [form, setForm] = useState<FormValues>(loadForm);
  const [tab, setTab] = useState<Tab>("params");
  const [waveform, setWaveform] = useState<[number, number][] | null>(null);
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<JobStatusKind | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [vizKey, setVizKey] = useState(0);

  const [jobs, setJobs] = useState<JobSummary[]>([]);
  const [jobsLoading, setJobsLoading] = useState(false);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [viewMode, setViewMode] = useState<ViewMode>("single");

  const subRef = useRef<LogSubscription | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(form));
    } catch {
      /* ignore */
    }
  }, [form]);

  useEffect(() => () => subRef.current?.close(), []);

  const fetchJobs = useCallback(() => {
    setJobsLoading(true);
    api
      .listJobs()
      .then(setJobs)
      .catch((e) => setError(String(e)))
      .finally(() => setJobsLoading(false));
  }, []);

  // ジョブタブ表示時に一覧を取得
  useEffect(() => {
    if (tab === "jobs") fetchJobs();
  }, [tab, fetchJobs]);

  const onChange = (key: string, value: string) =>
    setForm((f) => ({ ...f, [key]: value }));

  const appendLog = (line: string) =>
    setLog((l) => {
      const next = [...l, line];
      return next.length > MAX_LOG ? next.slice(-MAX_LOG) : next;
    });

  const handleRun = async () => {
    let params;
    try {
      params = buildParams(form);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setTab("params");
      return;
    }
    if (form.voltageMode === "custom") {
      if (!waveform) {
        setError("カスタム波形モードでは CSV をアップロードしてください");
        setTab("params");
        return;
      }
      params.customWaveformData = waveform;
    }
    const t = form.threads?.trim() ? Number(form.threads) : undefined;
    const threads = t && Number.isFinite(t) && t > 0 ? t : null;

    setError(null);
    setLog([]);
    setRunning(true);
    setStatus("running");
    setTab("run");

    try {
      const { jobId } = await api.createJob({
        params,
        mode: (form.mode as RunMode) || "init_measure",
        label: form.label?.trim() || null,
        threads,
      });
      setActiveJobId(jobId);
      setViewMode("single");
      appendLog(`>> ジョブ投入: ${jobId}`);

      subRef.current?.close();
      subRef.current = subscribeLogs(
        jobId,
        (line) => appendLog(line),
        (p) => {
          setRunning(false);
          const st: JobStatusKind = p.stopped ? "stopped" : p.code === 0 ? "done" : "failed";
          setStatus(st);
          appendLog(p.stopped ? ">> 停止しました" : `>> 完了 (終了コード ${p.code})`);
          setVizKey((v) => v + 1);
        },
      );
    } catch (e) {
      setRunning(false);
      setStatus("failed");
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      appendLog(`>> エラー: ${msg}`);
    }
  };

  const handleStop = () => {
    if (activeJobId) api.stopJob(activeJobId).catch((e) => appendLog(`>> ${e}`));
  };

  const handleWaveform = (data: [number, number][] | null, err?: string) => {
    setWaveform(data);
    if (err) setError(err);
  };

  const resetForm = () => {
    if (confirm("計算条件を既定値に戻しますか？")) {
      setForm({ ...DEFAULT_FORM });
      setWaveform(null);
    }
  };

  // --- ジョブ一覧操作 ---
  const toggleSelect = (id: string) =>
    setSelectedIds((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));

  const viewJob = (id: string) => {
    setActiveJobId(id);
    setViewMode("single");
    setVizKey((v) => v + 1);
    setTab("visualize");
  };

  const startCompare = () => {
    if (selectedIds.length >= 2) {
      setViewMode("compare");
      setTab("visualize");
    }
  };

  const deleteJob = (id: string) => {
    if (!confirm("このジョブと作業データを削除しますか？")) return;
    api
      .deleteJob(id)
      .then(() => {
        setSelectedIds((s) => s.filter((x) => x !== id));
        if (activeJobId === id) setActiveJobId(null);
        fetchJobs();
      })
      .catch((e) => setError(String(e)));
  };

  const stopJob = (id: string) => {
    api
      .stopJob(id)
      .then(() => setTimeout(fetchJobs, 400))
      .catch((e) => setError(String(e)));
  };

  const compareJobs = selectedIds
    .map((id) => jobs.find((j) => j.id === id))
    .filter((j): j is JobSummary => !!j);

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <span className="mark">
            Rust-PIC
          </span>
          <span className="sub">Plasma Simulation Console</span>
        </div>
        <nav className="tabs">
          <button className={tab === "params" ? "active" : ""} onClick={() => setTab("params")}>
            計算条件
          </button>
          <button className={tab === "run" ? "active" : ""} onClick={() => setTab("run")}>
            実行
          </button>
          <button className={tab === "jobs" ? "active" : ""} onClick={() => setTab("jobs")}>
            ジョブ
          </button>
          <button className={tab === "visualize" ? "active" : ""} onClick={() => setTab("visualize")}>
            可視化
          </button>
        </nav>
        <div className="header-actions">
          {tab === "params" && (
            <button className="btn ghost" onClick={resetForm} disabled={running}>
              既定値に戻す
            </button>
          )}
          <button className="btn primary" onClick={handleRun} disabled={running}>
            ▶ 計算開始
          </button>
        </div>
      </header>

      <main className="app-content">
        {error && (tab === "params" || tab === "jobs") && (
          <div className="notice error">{error}</div>
        )}

        {tab === "params" && (
          <ParamForm
            form={form}
            onChange={onChange}
            disabled={running}
            waveform={waveform}
            onWaveform={handleWaveform}
          />
        )}
        {tab === "run" && (
          <RunPanel form={form} status={status} running={running} log={log} onStop={handleStop} />
        )}
        {tab === "jobs" && (
          <JobsList
            jobs={jobs}
            selectedIds={selectedIds}
            loading={jobsLoading}
            onToggleSelect={toggleSelect}
            onView={viewJob}
            onStop={stopJob}
            onDelete={deleteJob}
            onCompare={startCompare}
            onRefresh={fetchJobs}
          />
        )}
        {tab === "visualize" &&
          (viewMode === "compare" && compareJobs.length >= 2 ? (
            <Compare jobs={compareJobs} />
          ) : (
            <Visualize jobId={activeJobId} refreshKey={vizKey} />
          ))}
      </main>
    </div>
  );
}
