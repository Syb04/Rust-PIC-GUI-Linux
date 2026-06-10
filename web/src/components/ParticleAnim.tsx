import { useEffect, useMemo, useRef, useState } from "react";
import Plotly from "plotly.js-dist-min";
import type { AnimData, AnimFrame } from "../types";

interface Props {
  anim: AnimData;
}

const CONFIG = { responsive: true, displaylogo: false };
const FONT = { color: "#aeb9cc", family: "'IBM Plex Mono', monospace", size: 12 };
const GRID = "rgba(86,104,135,0.18)";
const ZERO = "rgba(86,104,135,0.4)";
const ELECTRON = "#36e0d6";
const ION = "#ff9d4d";

function velocityRange(frames: AnimFrame[], key: "evx" | "ivx"): [number, number] {
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const frame of frames) {
    for (const value of frame[key]) {
      if (!Number.isFinite(value)) continue;
      min = Math.min(min, value);
      max = Math.max(max, value);
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [-1, 1];
  if (min === max) {
    const pad = Math.max(Math.abs(min) * 0.1, 1);
    return [min - pad, max + pad];
  }
  const pad = (max - min) * 0.05;
  return [min - pad, max + pad];
}

function axisStyle(title: string, range?: [number, number]) {
  return {
    title: { text: title, font: { size: 12 } },
    range,
    gridcolor: GRID,
    zerolinecolor: ZERO,
    linecolor: GRID,
    tickfont: { size: 11 },
  };
}

export function ParticleAnim({ anim }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const frameCount = anim.frames.length;
  const [frameIndex, setFrameIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [fps, setFps] = useState(20);

  const electronRange = useMemo(() => velocityRange(anim.frames, "evx"), [anim.frames]);
  const ionRange = useMemo(() => velocityRange(anim.frames, "ivx"), [anim.frames]);
  const xRange: [number, number] = useMemo(() => {
    const gap = Number.isFinite(anim.gapM) && anim.gapM > 0 ? anim.gapM : 1;
    return [0, gap];
  }, [anim.gapM]);

  useEffect(() => {
    setFrameIndex(0);
    setPlaying(false);
  }, [anim]);

  useEffect(() => {
    setFrameIndex((current) => Math.min(current, Math.max(frameCount - 1, 0)));
  }, [frameCount]);

  useEffect(() => {
    if (!playing || frameCount <= 1) return;
    const timer = window.setInterval(() => {
      setFrameIndex((current) => (current + 1) % frameCount);
    }, 1000 / fps);
    return () => window.clearInterval(timer);
  }, [playing, fps, frameCount]);

  useEffect(() => {
    return () => {
      if (ref.current) Plotly.purge(ref.current);
    };
  }, []);

  const frame = anim.frames[frameIndex];
  const timeLabel = frame ? `t/T = ${frame.t.toFixed(2)}` : "t/T = --";

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!frame) {
      Plotly.purge(el);
      el.textContent = "アニメーションフレームがありません";
      return;
    }

    const traces = [
      {
        x: frame.ex,
        y: frame.evx,
        type: "scatter",
        mode: "markers",
        name: "電子",
        marker: { color: ELECTRON, size: 3, opacity: 0.82 },
        hovertemplate: "x=%{x:.3e} m<br>vx=%{y:.3e} m/s<extra>電子</extra>",
      },
      {
        x: frame.ix,
        y: frame.ivx,
        type: "scatter",
        mode: "markers",
        name: "正イオン",
        xaxis: "x2",
        yaxis: "y2",
        marker: { color: ION, size: 3, opacity: 0.82 },
        hovertemplate: "x=%{x:.3e} m<br>vx=%{y:.3e} m/s<extra>正イオン</extra>",
      },
    ];

    const layout = {
      paper_bgcolor: "rgba(0,0,0,0)",
      plot_bgcolor: "rgba(8,12,20,0.35)",
      font: FONT,
      margin: { l: 78, r: 28, t: 24, b: 58 },
      autosize: true,
      showlegend: true,
      legend: { orientation: "h", y: -0.18, font: { size: 11 } },
      xaxis: { ...axisStyle("x [m]", xRange), domain: [0, 1], anchor: "y" },
      yaxis: { ...axisStyle("電子 vx [m/s]", electronRange), domain: [0.57, 1], anchor: "x" },
      xaxis2: { ...axisStyle("x [m]", xRange), domain: [0, 1], anchor: "y2" },
      yaxis2: { ...axisStyle("正イオン vx [m/s]", ionRange), domain: [0, 0.43], anchor: "x2" },
    };

    Plotly.react(el, traces, layout, CONFIG);
  }, [frame, electronRange, ionRange, xRange]);

  if (frameCount === 0) {
    return <div className="notice error">anim.json にフレームがありません</div>;
  }

  return (
    <div className="particle-anim">
      <div className="particle-anim-toolbar">
        <button className="btn ghost" type="button" onClick={() => setPlaying((v) => !v)}>
          {playing ? "一時停止" : "再生"}
        </button>
        <label className="ctrl particle-anim-slider">
          フレーム
          <input
            type="range"
            min={0}
            max={Math.max(frameCount - 1, 0)}
            value={frameIndex}
            onChange={(e) => setFrameIndex(Number(e.target.value))}
          />
          <span>
            {frameIndex + 1}/{anim.framesPerCycle || frameCount}
          </span>
        </label>
        <label className="ctrl">
          速度
          <select value={fps} onChange={(e) => setFps(Number(e.target.value))}>
            <option value={10}>10 fps</option>
            <option value={20}>20 fps</option>
            <option value={40}>40 fps</option>
          </select>
        </label>
        <span className="particle-anim-time">{timeLabel}</span>
      </div>
      <div className="particle-anim-plot" ref={ref} />
    </div>
  );
}
