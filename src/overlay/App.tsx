import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../shared/ipc";
import { LAYOUT_RESET_EVENT, loadLayout, saveLayout } from "../shared/layout";
import type { Layout, OverlaySettings, WidgetId, WidgetRect } from "../shared/types";
import { useOverlayStatus } from "../shared/useOverlayStatus";
import { useSettings } from "../shared/useSettings";
import { WidgetFrame } from "./WidgetFrame";
import { AlertsWidget } from "./widgets/AlertsWidget";
import { ChatWidget } from "./widgets/ChatWidget";
import { ObsStatusWidget } from "./widgets/ObsStatusWidget";

function useViewport() {
  const [size, setSize] = useState({ w: window.innerWidth, h: window.innerHeight });
  useEffect(() => {
    const onResize = () => setSize({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return size;
}

/** Terapkan pengaturan tampilan sebagai CSS variable (lihat overlay.css). */
function useAppearance(overlay: OverlaySettings | undefined) {
  useEffect(() => {
    if (!overlay) return;
    const style = document.documentElement.style;
    style.setProperty("--widget-opacity", String(overlay.widgetOpacity));
    style.setProperty("--font-scale", String(overlay.fontScale));
    style.setProperty("--chat-fade-delay", `${overlay.chatFadeSeconds}s`);
  }, [overlay]);
}

export default function App() {
  const status = useOverlayStatus();
  const editMode = status?.editMode ?? false;
  const viewport = useViewport();
  const [layout, setLayout] = useState<Layout | null>(null);
  const { settings } = useSettings();
  useAppearance(settings?.overlay);
  const noFade = settings?.overlay.chatFadeSeconds === 0;

  useEffect(() => {
    loadLayout().then(setLayout);
    const unlisten = listen(LAYOUT_RESET_EVENT, () => loadLayout().then(setLayout));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Esc = kembali ke mode pasif (fokus dikembalikan ke game oleh Rust).
  useEffect(() => {
    if (!editMode) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") ipc.overlaySetEditMode(false).catch(console.error);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [editMode]);

  const updateWidget = useCallback((id: WidgetId, rect: WidgetRect) => {
    setLayout((prev) => {
      if (!prev) return prev;
      const next = { ...prev, [id]: rect };
      void saveLayout(next);
      return next;
    });
  }, []);

  if (!layout) return null;

  return (
    <div className={["overlay-root", editMode && "edit", noFade && "no-fade"].filter(Boolean).join(" ")}>
      {editMode && (
        <>
          <div className="edit-backdrop" />
          <div className="edit-hint">
            <strong>MODE EDIT</strong> · seret &amp; ubah ukuran widget · <kbd>Esc</kbd> atau{" "}
            <kbd>{settings?.hotkeys.toggleEdit ?? "hotkey mode edit"}</kbd> untuk kembali ke game
            <span className={status?.contentProtected ? "hint-chip ok" : "hint-chip warn"}>
              {status?.contentProtected
                ? "Tersembunyi dari capture"
                : "TERLIHAT di capture"}
            </span>
          </div>
        </>
      )}

      <WidgetFrame
        id="obsStatus"
        title="Status OBS"
        rect={layout.obsStatus}
        viewport={viewport}
        editMode={editMode}
        minWidth={260}
        minHeight={36}
        onChange={updateWidget}
      >
        <ObsStatusWidget editMode={editMode} />
      </WidgetFrame>

      <WidgetFrame
        id="chat"
        title="Live Chat"
        rect={layout.chat}
        viewport={viewport}
        editMode={editMode}
        minWidth={200}
        minHeight={120}
        onChange={updateWidget}
      >
        <ChatWidget editMode={editMode} />
      </WidgetFrame>

      <WidgetFrame
        id="alerts"
        title="Alert"
        rect={layout.alerts}
        viewport={viewport}
        editMode={editMode}
        minWidth={240}
        minHeight={60}
        onChange={updateWidget}
      >
        <AlertsWidget editMode={editMode} />
      </WidgetFrame>
    </div>
  );
}
