import { useInspectorStore } from "../store/inspectorStore";
import type { PerfSample } from "../api/types";

const WIDTH = 300;
const HEIGHT = 100;
const PADDING = 4;

/** SVG sparkline showing resource metrics over time. */
export function PerformanceChart() {
  const perfHistory = useInspectorStore((s) => s.perfHistory);

  if (perfHistory.length < 2) return null;

  const maxPss = Math.max(...perfHistory.map((s) => s.totalPssKb), 1);
  const maxCpu = Math.max(...perfHistory.map((s) => s.cpuPercent), 1);

  return (
    <div className="performance-chart">
      <div className="perf-header">
        <span className="perf-title">Performance</span>
        <span className="perf-legend">
          <span className="legend-dot" style={{ background: "#4ade80" }} /> Java
          <span className="legend-dot" style={{ background: "#60a5fa" }} /> Native
          <span className="legend-dot" style={{ background: "#fb923c" }} /> PSS
          <span className="legend-dot" style={{ background: "#f87171" }} /> CPU
        </span>
      </div>
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        className="perf-svg"
        preserveAspectRatio="none"
      >
        <Polyline data={perfHistory} getValue={(s) => s.javaHeapKb} max={maxPss} color="#4ade80" />
        <Polyline data={perfHistory} getValue={(s) => s.nativeHeapKb} max={maxPss} color="#60a5fa" />
        <Polyline data={perfHistory} getValue={(s) => s.totalPssKb} max={maxPss} color="#fb923c" />
        <Polyline data={perfHistory} getValue={(s) => s.cpuPercent} max={maxCpu} color="#f87171" />
      </svg>
      <div className="perf-values">
        <span>PSS: {(perfHistory[perfHistory.length - 1].totalPssKb / 1024).toFixed(1)}MB</span>
        <span>CPU: {perfHistory[perfHistory.length - 1].cpuPercent.toFixed(1)}%</span>
      </div>
    </div>
  );
}

function Polyline({
  data,
  getValue,
  max,
  color,
}: {
  data: PerfSample[];
  getValue: (s: PerfSample) => number;
  max: number;
  color: string;
}) {
  const points = data
    .map((sample, i) => {
      const x = PADDING + ((WIDTH - 2 * PADDING) * i) / (data.length - 1);
      const y = HEIGHT - PADDING - ((HEIGHT - 2 * PADDING) * getValue(sample)) / max;
      return `${x},${y}`;
    })
    .join(" ");

  return (
    <polyline
      points={points}
      fill="none"
      stroke={color}
      strokeWidth="1.5"
      strokeLinejoin="round"
    />
  );
}
