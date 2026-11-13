import { Checklist, PHASE0_ITEMS, PHASE2_ITEMS } from "../Checklist";

export function TestingTab() {
  return (
    <div className="grid">
      <section className="card wide">
        <h2>Checklist Fase 2 · OBS</h2>
        <Checklist
          storageKey="phase2-checklist"
          items={PHASE2_ITEMS}
          doneText="Fase 2 selesai, lanjut ke Fase 3 (chat YouTube)."
        />
      </section>

      <section className="card wide">
        <h2>Checklist Fase 0 · overlay</h2>
        <Checklist
          storageKey="phase0-checklist"
          items={PHASE0_ITEMS}
          doneText="Overlay layak dipakai di game."
        />
      </section>

      <section className="card wide tips">
        <h2>Tips</h2>
        <ul>
          <li>
            Jalankan game dalam mode <strong>Borderless / Windowed Fullscreen</strong>. Overlay
            tidak bisa tampil di atas game <em>exclusive fullscreen</em>.
          </li>
          <li>
            Untuk mengukur dampak FPS, bandingkan FPS game dengan overlay tampil vs tersembunyi
            (<kbd>Ctrl+Shift+O</kbd>), lalu dengan aplikasi ditutup sepenuhnya.
          </li>
          <li>
            Mau mencoba tanpa membuka OBS? Jalankan server OBS tiruan dari folder{" "}
            <code>src-tauri</code>: <code>cargo run --example mock_obs -- --live</code> (port 4455,
            tanpa password).
          </li>
          <li>Chat di overlay masih DEMO; chat YouTube asli dibuat di Fase 3.</li>
        </ul>
      </section>
    </div>
  );
}
