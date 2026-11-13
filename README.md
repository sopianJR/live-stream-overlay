# LiveStream Overlay (prototype)

Aplikasi desktop Windows untuk streamer satu monitor: **live chat YouTube** dan **status OBS**
tampil sebagai overlay di atas game. Overlay tembus klik, tidak mencuri fokus, dan tidak ikut
tertangkap di stream.

Dibangun dengan **Tauri v2 (Rust) + React + TypeScript**. Semua servis jangka panjang
(OBS, chat, alert) berjalan di Rust, sedangkan React hanya menampilkan state.

> Status: **Fase 2, koneksi OBS selesai.** Status OBS sudah asli; chat masih DEMO (chat YouTube asli di Fase 3).

## Fitur saat ini

- Overlay transparan, tembus klik, tidak mencuri fokus, tersembunyi dari capture
- Widget status OBS & chat yang bisa diseret/diubah ukuran (mode edit)
- **Status OBS asli** lewat obs-websocket v5:
  - LIVE/OFFLINE/RECONNECTING + durasi, bitrate (rata-rata 3 detik), drop % (10 detik terakhir),
    FPS, status rekaman, scene aktif, mic mute
  - tersambung ulang otomatis (1, 2, 4, 8, lalu tiap 10 detik); berhenti mencoba jika password salah
  - kontrol di mode edit: mulai/stop stream (**harus ditahan**), rekam, mute mic, ganti scene
  - notifikasi singkat di overlay untuk hasil hotkey
- Dashboard bertab:
  - **Overlay:** status, pilih monitor, kepekatan latar, ukuran huruf, durasi pudar chat
  - **Koneksi:** status koneksi OBS, alamat & password OBS, input mic yang dipantau, sumber chat YouTube
  - **Hotkey:** ubah kombinasi tombol (direkam langsung, wajib pakai modifier)
  - **Pengujian:** checklist Fase 2 (OBS) dan Fase 0 (overlay)
- System tray, single-instance, dashboard dihancurkan saat ditutup (hemat RAM)

## Data & log

| Apa | Lokasi |
|---|---|
| Pengaturan | `%APPDATA%\com.livestreamapps.overlay\settings.json` |
| Layout widget | `%APPDATA%\com.livestreamapps.overlay\layout.json` |
| Password OBS | Windows Credential Manager, target `LiveStreamOverlay/obs-websocket` |
| Log | `%LOCALAPPDATA%\com.livestreamapps.overlay\logs\` |

## Prasyarat

- Windows 10 versi 2004 atau lebih baru (wajib untuk fitur "tersembunyi dari capture")
- Node.js 20+
- Rust stable (`rustup`, toolchain `stable-x86_64-pc-windows-msvc`)
- Visual Studio 2022 Build Tools dengan workload **Desktop development with C++**
- WebView2 Runtime (sudah bawaan Windows 11)

## Menjalankan

```bash
npm install
npm run tauri dev
```

Build installer (NSIS):

```bash
npm run tauri build
```

Saat aplikasi berjalan, overlay langsung tampil dalam **mode pasif** dan dashboard terbuka.
Menutup dashboard tidak mematikan aplikasi; aplikasi tetap berjalan di system tray.

## Hotkey

Default (bisa diubah di dashboard → Hotkey):

| Hotkey | Fungsi |
|---|---|
| `Ctrl+Shift+O` | Tampilkan / sembunyikan overlay |
| `Ctrl+Shift+E` | Masuk / keluar mode edit (seret widget, kontrol OBS) |
| `Ctrl+Shift+M` | Mute / unmute mic di OBS |
| `Ctrl+Shift+R` | Mulai / hentikan rekaman OBS |
| `Esc` | Keluar dari mode edit; fokus dikembalikan ke game |

Hotkey bersifat global selama aplikasi berjalan, jadi kombinasi yang sama tidak sampai ke aplikasi
lain (mis. `Ctrl+Shift+R` = hard reload di browser). Ubah di dashboard jika bentrok.
Start/stop stream sengaja tidak diberi hotkey.

## Test

```bash
cd src-tauri && cargo test
```

Unit test mencakup parser hotkey, validasi pengaturan, penyimpanan Credential Manager
(memakai target khusus test yang langsung dihapus), perhitungan bitrate/drop, pengolahan event OBS,
dan koneksi ke **server OBS tiruan** (password benar/salah/kosong, polling, kontrol, OBS ditutup).

### Mencoba tanpa OBS

Server obs-websocket tiruan bisa dijalankan sendiri, mis. untuk melihat overlay dalam keadaan live:

```bash
cd src-tauri && cargo run --example mock_obs -- --live --drop 0.5
```

Opsi: `--port 4455`, `--password RAHASIA`, `--live`, `--drop <persen>`, `--kbps <bitrate>`.
Pastikan WebSocket server OBS asli tidak aktif di port yang sama.

## Checklist Fase 2 (OBS)

Checklist yang sama ada di dashboard → Pengujian dan bisa dicentang di sana.

1. **(a) Terhubung:** aktifkan WebSocket server di OBS, isi password di tab Koneksi → "Terhubung".
2. **(b) Password salah:** muncul "Password OBS salah" dan aplikasi berhenti mencoba; password benar → terhubung.
3. **(c) Angka akurat:** mulai stream *unlisted*; bitrate, drop %, dan FPS mendekati dock Stats OBS (±5%).
4. **(d) Pulih sendiri:** tutup OBS → "OBS belum terhubung"; buka lagi → tersambung otomatis ±10 detik.
5. **(e) Hotkey di game:** `Ctrl+Shift+M` / `Ctrl+Shift+R` saat game fokus; ikon berubah + notifikasi.
6. **(f) Kontrol mode edit:** ganti scene, mute mic, rekam; stop stream hanya jalan jika ditahan 1,5 detik.

## Checklist Fase 0 (overlay)

1. **(a) Tampil di atas game:** jalankan 2–3 game dalam mode *Borderless / Windowed Fullscreen*.
2. **(b) Klik tembus:** di mode pasif, klik di area widget harus sampai ke game.
3. **(c) Tidak mencuri fokus:** game tidak ter-minimize dan keyboard tetap ke game.
4. **(d) Tidak terlihat di OBS:** tambahkan *Display Capture* di OBS. Overlay tidak boleh terlihat
   di preview. Matikan toggle proteksi di dashboard untuk membandingkan.
5. **(e) Hotkey di dalam game:** `Ctrl+Shift+O` / `Ctrl+Shift+E` berfungsi saat game fokus.
6. **(f) Dampak FPS < 3%:** bandingkan FPS game dengan overlay tampil vs tersembunyi.

Jika (f) gagal, rencana cadangannya adalah satu window kecil per widget, bukan satu window
selebar layar.

## Struktur

```
src/
  dashboard/   UI pengaturan & checklist (index.html)
  overlay/     UI overlay transparan + widget (overlay.html)
  shared/      tipe, wrapper IPC, penyimpanan layout
src-tauri/src/
  overlay/        window overlay, monitor, mode pasif/edit, flag Win32, watchdog topmost
  obs/            koneksi OBS: reconnect, polling & event (mod.rs), snapshot status (status.rs),
                  bitrate & drop % (stats.rs)
  settings.rs     struktur & validasi pengaturan, simpan/muat
  accelerator.rs  parser & normalisasi teks hotkey
  hotkeys.rs      hotkey global yang bisa diubah (+ jeda saat direkam)
  secrets.rs      Windows Credential Manager
  commands.rs     command untuk frontend
  lib.rs          setup aplikasi, tray, plugin, log
src-tauri/tests/support/mock_obs.rs   server obs-websocket tiruan (test + example)
src-tauri/examples/mock_obs.rs        `cargo run --example mock_obs`
```

## Rencana berikutnya

- ~~**Fase 1:** pengaturan, pemilihan monitor, penyimpanan rahasia~~ selesai
- ~~**Fase 2:** OBS WebSocket (status asli + kontrol)~~ selesai
- **Fase 3:** chat YouTube via InnerTube + jumlah penonton
- **Fase 4:** alert masalah stream
- **Fase 5:** balas chat (OAuth + YouTube Data API)
- **Fase 6:** polishing & installer
