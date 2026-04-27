/**
 * Sparkline — SVG mini chart. Pure: data + viewport in, polyline +
 * fill area + last-point dot out. No interaction; the parent card
 * owns delta % and label rendering.
 *
 * Min/max derive from the data, with a small floor so a flat-zero
 * series still draws a visible baseline instead of NaN.
 */
import { useMemo } from 'react';

interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  fillOpacity?: number;
  strokeWidth?: number;
}

export function Sparkline({
  data,
  width = 120,
  height = 32,
  color = 'var(--accent)',
  fillOpacity = 0.18,
  strokeWidth = 1.5,
}: SparklineProps) {
  const { line, area, lastX, lastY } = useMemo(() => {
    if (data.length === 0) {
      return { line: '', area: '', lastX: 0, lastY: height };
    }
    const min = Math.min(...data);
    const max = Math.max(...data);
    const span = Math.max(max - min, 1e-6);
    const stepX = data.length > 1 ? width / (data.length - 1) : 0;
    const points = data.map((v, i) => {
      const x = i * stepX;
      const y = height - ((v - min) / span) * (height - 2) - 1;
      return [x, y] as const;
    });
    const linePath = points.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`).join(' ');
    const areaPath = `${linePath} L${(points[points.length - 1][0]).toFixed(2)},${height} L0,${height} Z`;
    const [lx, ly] = points[points.length - 1];
    return { line: linePath, area: areaPath, lastX: lx, lastY: ly };
  }, [data, width, height]);

  if (data.length === 0) {
    return (
      <svg className="sparkline empty" width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
        <line x1={0} y1={height - 1} x2={width} y2={height - 1} stroke="var(--bg-3)" strokeDasharray="2 3" />
      </svg>
    );
  }

  return (
    <svg className="sparkline" width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      <path d={area} fill={color} fillOpacity={fillOpacity} stroke="none" />
      <path d={line} fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
      <circle cx={lastX} cy={lastY} r={2.5} fill={color} />
    </svg>
  );
}
