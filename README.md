# LiveStream Overlay (prototype)

Aplikasi desktop Windows untuk streamer satu monitor: **live chat YouTube** dan **status OBS**
tampil sebagai overlay di atas game. Overlay tembus klik, tidak mencuri fokus, dan tidak ikut
tertangkap di stream.

Dibangun dengan **Tauri v2 (Rust) + React + TypeScript**. Semua servis jangka panjang
(OBS, chat, alert) berjalan di Rust, sedangkan React hanya menampilkan state.

> Status: **Fase 1, fondasi selesai.** Data chat dan status OBS masih DEMO (koneksi asli di Fase 2–3).

## Fitur saat ini

- Overlay transparan, tembus klik, tidak mencuri fokus, tersembunyi dari capture
- Widget status OBS & chat yang bisa diseret/diubah ukuran (mode edit)
- Dashboard bertab:
  - **Overlay:** status, pilih monitor, kepekatan latar, ukuran huruf, durasi pudar chat
  - **Koneksi:** alamat OBS WebSocket, password OBS, sumber chat YouTube
  - **Hotkey:** ubah kombinasi tombol (direkam langsung, wajib pakai modifier)
  - **Pengujian:** checklist Fase 0
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
| `Ctrl+Shift+E` | Masuk / keluar mode edit (seret & ubah ukuran widget) |
| `Esc` | Keluar dari mode edit; fokus dikembalikan ke game |

## Test

```bash
cd src-tauri && cargo test
```

Unit test mencakup parser hotkey, validasi pengaturan, dan penyimpanan Credential Manager
(memakai target khusus test yang langsung dihapus).

## Checklist Fase 0

Checklist yang sama ada di dashboard dan bisa dicentang di sana.

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
  settings.rs     struktur & validasi pengaturan, simpan/muat
  accelerator.rs  parser & normalisasi teks hotkey
  hotkeys.rs      hotkey global yang bisa diubah (+ jeda saat direkam)
  secrets.rs      Windows Credential Manager
  commands.rs     command untuk frontend
  lib.rs          setup aplikasi, tray, plugin, log
```

## Rencana berikutnya

- ~~**Fase 1:** pengaturan, pemilihan monitor, penyimpanan rahasia~~ selesai
- **Fase 2:** OBS WebSocket (status asli + kontrol)
- **Fase 3:** chat YouTube via InnerTube + jumlah penonton
- **Fase 4:** alert masalah stream
- **Fase 5:** balas chat (OAuth + YouTube Data API)
- **Fase 6:** polishing & installer
