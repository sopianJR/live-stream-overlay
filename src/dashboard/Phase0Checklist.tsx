import { useState } from "react";

const STORAGE_KEY = "phase0-checklist";

const ITEMS = [
  {
    id: "a",
    title: "Overlay tampil di atas game",
    how: "Buka 2–3 game dalam mode borderless. Widget status & chat harus terlihat di atas game.",
  },
  {
    id: "b",
    title: "Klik tembus ke game",
    how: "Di mode pasif, klik/aim di area widget. Input harus sampai ke game, bukan ke overlay.",
  },
  {
    id: "c",
    title: "Tidak mencuri fokus",
    how: "Saat overlay muncul/berganti isi, game tidak ter-minimize dan input keyboard tetap ke game.",
  },
  {
    id: "d",
    title: "Tidak terlihat di OBS",
    how: "Tambahkan Display Capture di OBS. Overlay tidak boleh tampak di preview. Matikan proteksi untuk membandingkan.",
  },
  {
    id: "e",
    title: "Hotkey berfungsi saat game fokus",
    how: "Tekan Ctrl+Shift+O dan Ctrl+Shift+E di dalam game. Setelah Esc, fokus kembali ke game.",
  },
  {
    id: "f",
    title: "Dampak FPS < 3%",
    how: "Bandingkan FPS game (mis. via OBS Stats atau overlay FPS game) dengan overlay tampil vs tersembunyi.",
  },
] as const;

type Checked = Record<string, boolean>;

function readChecked(): Checked {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Checked) : {};
  } catch {
    return {};
  }
}

function writeChecked(value: Checked) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // Penyimpanan tidak tersedia; checklist tetap berfungsi selama sesi ini.
  }
}

export function Phase0Checklist() {
  const [checked, setChecked] = useState<Checked>(readChecked);

  const toggle = (id: string) => {
    setChecked((prev) => {
      const next = { ...prev, [id]: !prev[id] };
      writeChecked(next);
      return next;
    });
  };

  const done = ITEMS.filter((i) => checked[i.id]).length;

  return (
    <>
      <p className="muted">
        {done} dari {ITEMS.length} lulus.
        {done === ITEMS.length && " Fase 0 selesai, lanjut ke Fase 1."}
      </p>
      <ul className="checklist">
        {ITEMS.map((item) => (
          <li key={item.id} className={checked[item.id] ? "done" : ""}>
            <label>
              <input type="checkbox" checked={!!checked[item.id]} onChange={() => toggle(item.id)} />
              <span>
                <strong>
                  ({item.id}) {item.title}
                </strong>
                <small>{item.how}</small>
              </span>
            </label>
          </li>
        ))}
      </ul>
    </>
  );
}
