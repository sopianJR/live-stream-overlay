import { Phase0Checklist } from "../Phase0Checklist";

export function TestingTab() {
  return (
    <div className="grid">
      <section className="card wide">
        <h2>Checklist pengujian Fase 0</h2>
        <Phase0Checklist />
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
          <li>Data chat dan status OBS masih DEMO; koneksi asli dibuat di Fase 2–3.</li>
        </ul>
      </section>
    </div>
  );
}
