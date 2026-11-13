import { useEffect, useState } from "react";

// DEMO Fase 0: angka disimulasikan. Fase 2 menggantinya dengan data OBS WebSocket asli.

interface DemoStats {
  seconds: number;
  bitrateKbps: number;
  dropPct: number;
  fps: number;
  micMuted: boolean;
}

function formatDuration(total: number): string {
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

function useDemoStats(): DemoStats {
  const [stats, setStats] = useState<DemoStats>({
    seconds: 0,
    bitrateKbps: 6000,
    dropPct: 0,
    fps: 60,
    micMuted: false,
  });

  useEffect(() => {
    const id = window.setInterval(() => {
      setStats((prev) => {
        const seconds = prev.seconds + 1;
        const bitrateKbps = Math.round(
          Math.min(6500, Math.max(5400, prev.bitrateKbps + (Math.random() - 0.5) * 300)),
        );
        // Sesekali simulasikan lonjakan dropped frames untuk melihat warna peringatan.
        const spike = seconds % 45 > 40;
        const dropPct = spike ? 1.5 + Math.random() * 2 : Math.random() * 0.2;
        const fps = spike ? 57 + Math.round(Math.random() * 2) : 60;
        const micMuted = seconds % 60 >= 50;
        return { seconds, bitrateKbps, dropPct, fps, micMuted };
      });
    }, 1000);
    return () => window.clearInterval(id);
  }, []);

  return stats;
}

export function ObsStatusWidget() {
  const s = useDemoStats();
  const dropClass = s.dropPct >= 5 ? "bad" : s.dropPct >= 1 ? "warn" : "ok";

  return (
    <div className="obs-bar">
      <span className="obs-live">
        <span className="live-dot" /> LIVE <span className="mono">{formatDuration(s.seconds)}</span>
      </span>
      <span className="obs-item mono">{(s.bitrateKbps / 1000).toFixed(1)} Mbps</span>
      <span className={`obs-item mono ${dropClass}`}>drop {s.dropPct.toFixed(1)}%</span>
      <span className="obs-item mono">{s.fps} fps</span>
      <span className={s.micMuted ? "obs-item bad" : "obs-item"}>
        {s.micMuted ? "🔇 mic mute" : "🎙 mic"}
      </span>
      <span className="demo-tag">DEMO</span>
    </div>
  );
}
