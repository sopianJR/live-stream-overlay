import { useId, useState, type KeyboardEvent, type PointerEvent } from "react";

export interface MinutePoint {
  minute: number;
  value: number | null;
}

const W = 600;
const H = 170;
const PAD = { top: 14, right: 96, bottom: 30, left: 8 };
/** Jarak minimum (unit viewBox) antara label maksimum dan label nilai terakhir. */
const LABEL_GAP = 22;

/**
 * Grafik garis satu seri per menit (penonton atau bitrate). Satu sumbu, garis 2px,
 * nilai terakhir dilabeli langsung, crosshair + tooltip saat hover/fokus keyboard.
 */
export function MinuteChart({
  title,
  points,
  format,
}: {
  title: string;
  points: MinutePoint[];
  format: (v: number) => string;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const titleId = useId();
  const values = points.map((p) => p.value).filter((v): v is number => v !== null);

  if (points.length < 2 || values.length === 0) {
    return (
      <figure className="minute-chart">
        <figcaption id={titleId}>{title}</figcaption>
        <p className="muted">Belum cukup data (butuh minimal 2 menit).</p>
      </figure>
    );
  }

  const max = Math.max(...values);
  const yMax = max > 0 ? max * 1.1 : 1;
  const first = points[0].minute;
  const last = points[points.length - 1].minute;
  const span = Math.max(1, last - first);
  const x = (minute: number) => PAD.left + ((minute - first) / span) * (W - PAD.left - PAD.right);
  const y = (v: number) => PAD.top + (1 - v / yMax) * (H - PAD.top - PAD.bottom);

  // Garis terputus pada menit tanpa data.
  const segments: string[] = [];
  let current = "";
  for (const p of points) {
    if (p.value === null) {
      if (current) segments.push(current);
      current = "";
      continue;
    }
    current += `${current ? "L" : "M"}${x(p.minute).toFixed(1)},${y(p.value).toFixed(1)}`;
  }
  if (current) segments.push(current);

  const lastPoint = [...points].reverse().find((p) => p.value !== null)!;
  const active = hover === null ? null : points[hover];

  const onMove = (e: PointerEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - rect.left) / rect.width) * W;
    let best = 0;
    for (let i = 1; i < points.length; i++) {
      if (Math.abs(x(points[i].minute) - px) < Math.abs(x(points[best].minute) - px)) best = i;
    }
    setHover(best);
  };

  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const step = e.key === "ArrowLeft" ? -1 : 1;
    setHover((h) => Math.min(points.length - 1, Math.max(0, (h ?? (step > 0 ? -1 : points.length)) + step)));
  };

  const tipLeft = active ? (x(active.minute) / W) * 100 : 0;

  return (
    <figure className="minute-chart">
      <figcaption id={titleId}>{title}</figcaption>
      <div className="minute-chart-plot">
        <svg
          viewBox={`0 0 ${W} ${H}`}
          role="img"
          aria-labelledby={titleId}
          tabIndex={0}
          onPointerMove={onMove}
          onPointerLeave={() => setHover(null)}
          onFocus={() => setHover((h) => h ?? points.length - 1)}
          onBlur={() => setHover(null)}
          onKeyDown={onKey}
        >
          <line className="chart-grid" x1={PAD.left} x2={W - PAD.right} y1={y(0)} y2={y(0)} />
          <line className="chart-grid" x1={PAD.left} x2={W - PAD.right} y1={y(max)} y2={y(max)} />
          <text className="chart-axis" x={W - PAD.right + 8} y={y(max) + 6}>
            maks {format(max)}
          </text>
          {segments.map((d) => (
            <path key={d} className="chart-line" d={d} />
          ))}
          <circle className="chart-dot" cx={x(lastPoint.minute)} cy={y(lastPoint.value!)} r={4} />
          {Math.abs(y(lastPoint.value!) - y(max)) >= LABEL_GAP && (
            <text className="chart-label" x={W - PAD.right + 8} y={y(lastPoint.value!) + 6}>
              {format(lastPoint.value!)}
            </text>
          )}
          <text className="chart-axis" x={PAD.left} y={H - 8}>
            menit {first}
          </text>
          <text className="chart-axis" x={W - PAD.right} y={H - 8} textAnchor="end">
            menit {last}
          </text>
          {active && (
            <>
              <line className="chart-crosshair" x1={x(active.minute)} x2={x(active.minute)} y1={PAD.top} y2={y(0)} />
              {active.value !== null && <circle className="chart-dot" cx={x(active.minute)} cy={y(active.value)} r={4} />}
            </>
          )}
        </svg>
        {active && (
          <div className="chart-tip" style={{ left: `${tipLeft}%` }} role="status">
            <strong>{active.value === null ? "–" : format(active.value)}</strong>
            <span>menit ke-{active.minute}</span>
          </div>
        )}
      </div>
      <details className="chart-table">
        <summary>Lihat data per menit</summary>
        <table>
          <thead>
            <tr>
              <th>Menit</th>
              <th>{title}</th>
            </tr>
          </thead>
          <tbody>
            {points.map((p) => (
              <tr key={p.minute}>
                <td>{p.minute}</td>
                <td>{p.value === null ? "–" : format(p.value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </figure>
  );
}
