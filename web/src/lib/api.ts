import type {
  CreateJobRequest,
  Diagnostic,
  FinishedPayload,
  JobSummary,
} from "../types";

// 同一オリジン（本番）/ vite proxy（開発）どちらでも相対パスで動く。
const BASE = "";

async function jsonOrThrow<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let msg = `HTTP ${res.status}`;
    try {
      const body = await res.json();
      if (body && typeof body.error === "string") msg = body.error;
    } catch {
      const t = await res.text().catch(() => "");
      if (t) msg = t;
    }
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

export const api = {
  createJob: (req: CreateJobRequest) =>
    fetch(`${BASE}/api/jobs`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(req),
    }).then((r) => jsonOrThrow<{ jobId: string }>(r)),

  listJobs: () => fetch(`${BASE}/api/jobs`).then((r) => jsonOrThrow<JobSummary[]>(r)),

  getJob: (id: string) =>
    fetch(`${BASE}/api/jobs/${id}`).then((r) => jsonOrThrow<JobSummary>(r)),

  stopJob: (id: string) =>
    fetch(`${BASE}/api/jobs/${id}/stop`, { method: "POST" }).then((r) => {
      if (!r.ok) throw new Error(`停止失敗: HTTP ${r.status}`);
    }),

  deleteJob: (id: string) =>
    fetch(`${BASE}/api/jobs/${id}`, { method: "DELETE" }).then((r) => {
      if (!r.ok) throw new Error(`削除失敗: HTTP ${r.status}`);
    }),

  listResults: (id: string) =>
    fetch(`${BASE}/api/jobs/${id}/results`).then((r) => jsonOrThrow<string[]>(r)),

  readDiagnostic: (id: string, name: string) =>
    fetch(`${BASE}/api/jobs/${id}/diag/${encodeURIComponent(name)}`).then((r) =>
      jsonOrThrow<Diagnostic>(r),
    ),
};

export interface LogSubscription {
  close: () => void;
}

/**
 * SSE でジョブログを購読する。
 * - log イベント: 1 行ずつ onLog へ。空行は無視する（finished 移行時の空イベント対策）。
 * - finished イベント: onFinished を呼び、自動的に接続を閉じる。
 */
export function subscribeLogs(
  id: string,
  onLog: (line: string) => void,
  onFinished: (p: FinishedPayload) => void,
  onError?: () => void,
): LogSubscription {
  const es = new EventSource(`${BASE}/api/jobs/${id}/logs`);

  es.addEventListener("log", (e) => {
    const line = (e as MessageEvent).data as string;
    if (line && line.length > 0) onLog(line);
  });

  es.addEventListener("finished", (e) => {
    try {
      const p = JSON.parse((e as MessageEvent).data) as FinishedPayload;
      onFinished(p);
    } catch {
      onFinished({ code: -2, stopped: false });
    }
    es.close();
  });

  es.onerror = () => {
    // ストリーム終了後の自然な切断と区別できないため、通知のみ
    onError?.();
  };

  return { close: () => es.close() };
}
