import { useEffect, useRef } from "react";
import { plotDiagnostic, purgePlot, type PlotOptions } from "../lib/plot";
import type { DiagSpec } from "../diagnostics";
import type { Diagnostic } from "../types";

interface Props {
  spec: DiagSpec;
  data: Diagnostic;
  opts?: PlotOptions;
}

/** Plotly 図を保持するホスト要素。spec/data/opts 変化で再描画する。 */
export function Plot({ spec, data, opts }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    plotDiagnostic(el, spec, data, opts ?? {});
    return () => {
      purgePlot(el);
    };
  }, [spec, data, opts]);

  return <div className="plot-host" ref={ref} />;
}
