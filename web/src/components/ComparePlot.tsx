import { useEffect, useRef } from "react";
import { plotCompareLines, purgePlot, type CompareSeries, type PlotOptions } from "../lib/plot";
import type { DiagSpec } from "../diagnostics";

interface Props {
  spec: DiagSpec;
  series: CompareSeries[];
  opts?: PlotOptions;
}

/** 複数ジョブの line 診断を重ね描きするホスト。 */
export function ComparePlot({ spec, series, opts }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    plotCompareLines(el, spec, series, opts ?? {});
    return () => purgePlot(el);
  }, [spec, series, opts]);

  return <div className="plot-host" ref={ref} />;
}
