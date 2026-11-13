import { useState } from "react";
import { ipc } from "../shared/ipc";
import { useSettings } from "../shared/useSettings";
import { ConnectionTab } from "./tabs/ConnectionTab";
import { HotkeyTab } from "./tabs/HotkeyTab";
import { OverlayTab } from "./tabs/OverlayTab";
import { TestingTab } from "./tabs/TestingTab";

const TABS = [
  { id: "overlay", label: "Overlay" },
  { id: "connection", label: "Koneksi" },
  { id: "hotkeys", label: "Hotkey" },
  { id: "testing", label: "Pengujian" },
] as const;

type TabId = (typeof TABS)[number]["id"];
const TAB_KEY = "dashboard-tab";

function readTab(): TabId {
  try {
    const saved = localStorage.getItem(TAB_KEY);
    if (TABS.some((t) => t.id === saved)) return saved as TabId;
  } catch {
    // Penyimpanan tidak tersedia; pakai tab pertama.
  }
  return "overlay";
}

export default function App() {
  const [tab, setTab] = useState<TabId>(readTab);
  const { settings, save } = useSettings();

  const selectTab = (id: TabId) => {
    setTab(id);
    try {
      localStorage.setItem(TAB_KEY, id);
    } catch {
      // abaikan
    }
  };

  return (
    <div className="app">
      <header className="header">
        <div>
          <h1>LiveStream Overlay</h1>
          <p className="subtitle">Live chat YouTube &amp; status OBS di atas game</p>
        </div>
        <span className="phase-chip">Prototype · Fase 2</span>
      </header>

      <nav className="tabs" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            className={tab === t.id ? "tab active" : "tab"}
            onClick={() => selectTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>

      <main>
        {!settings ? (
          <p className="muted">Memuat pengaturan…</p>
        ) : tab === "overlay" ? (
          <OverlayTab settings={settings} save={save} />
        ) : tab === "connection" ? (
          <ConnectionTab settings={settings} save={save} />
        ) : tab === "hotkeys" ? (
          <HotkeyTab settings={settings} save={save} />
        ) : (
          <TestingTab />
        )}
      </main>

      <footer className="footer">
        <span className="muted">Menutup jendela ini tidak mematikan aplikasi (tetap di system tray).</span>
        <button className="danger" onClick={() => ipc.appQuit().catch(console.error)}>
          Keluar aplikasi
        </button>
      </footer>
    </div>
  );
}
