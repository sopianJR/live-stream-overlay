import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { errorText, ipc } from "../shared/ipc";
import { useSettings } from "../shared/useSettings";
import { AlertTab } from "./tabs/AlertTab";
import { ConnectionTab } from "./tabs/ConnectionTab";
import { HotkeyTab } from "./tabs/HotkeyTab";
import { OverlayTab } from "./tabs/OverlayTab";
import { ONBOARDING_DONE_KEY, StartTab } from "./tabs/StartTab";
import { TestingTab } from "./tabs/TestingTab";

const TABS = [
  { id: "start", label: "Mulai" },
  { id: "overlay", label: "Overlay" },
  { id: "connection", label: "Koneksi" },
  { id: "alerts", label: "Alert" },
  { id: "hotkeys", label: "Hotkey" },
  { id: "testing", label: "Pengujian" },
] as const;

type TabId = (typeof TABS)[number]["id"];
const TAB_KEY = "dashboard-tab";

function storageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function storageSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Penyimpanan tidak tersedia; pilihan hanya berlaku selama jendela terbuka.
  }
}

/** Panduan "Mulai" dibuka otomatis sampai user menyelesaikan atau melewatinya. */
function initialTab(): TabId {
  if (storageGet(ONBOARDING_DONE_KEY) !== "1") return "start";
  const saved = storageGet(TAB_KEY);
  return TABS.some((t) => t.id === saved) ? (saved as TabId) : "overlay";
}

function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);
  return version;
}

export default function App() {
  const [tab, setTab] = useState<TabId>(initialTab);
  const { settings, save } = useSettings();
  const version = useAppVersion();
  const [footerError, setFooterError] = useState<string | null>(null);

  const selectTab = (id: TabId) => {
    setTab(id);
    storageSet(TAB_KEY, id);
  };

  const finishOnboarding = () => {
    storageSet(ONBOARDING_DONE_KEY, "1");
    selectTab("overlay");
  };

  const openLogs = () => {
    setFooterError(null);
    ipc.appOpenLogFolder().catch((e) => setFooterError(errorText(e)));
  };

  return (
    <div className="app">
      <header className="header">
        <div>
          <h1>LiveStream Overlay</h1>
          <p className="subtitle">Live chat YouTube &amp; status OBS di atas game</p>
        </div>
        <span className="phase-chip">Prototype{version ? ` v${version}` : ""}</span>
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
        ) : tab === "start" ? (
          <StartTab settings={settings} save={save} goTo={selectTab} onFinish={finishOnboarding} />
        ) : tab === "overlay" ? (
          <OverlayTab settings={settings} save={save} />
        ) : tab === "connection" ? (
          <ConnectionTab settings={settings} save={save} />
        ) : tab === "alerts" ? (
          <AlertTab settings={settings} save={save} />
        ) : tab === "hotkeys" ? (
          <HotkeyTab settings={settings} save={save} />
        ) : (
          <TestingTab settings={settings} />
        )}
      </main>

      <footer className="footer">
        <span className="muted">
          Menutup jendela ini tidak mematikan aplikasi (tetap di system tray).
          {footerError && <span className="msg error"> {footerError}</span>}
        </span>
        <div className="actions">
          <button className="secondary" onClick={openLogs} title="Riwayat koneksi & alert">
            Buka folder log
          </button>
          <button className="danger" onClick={() => ipc.appQuit().catch(console.error)}>
            Keluar aplikasi
          </button>
        </div>
      </footer>
    </div>
  );
}
