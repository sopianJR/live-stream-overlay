# LiveStream Overlay (prototype)

Aplikasi desktop Windows untuk streamer satu monitor: **live chat YouTube** dan **status OBS**
tampil sebagai overlay di atas game. Overlay tembus klik, tidak mencuri fokus, dan tidak ikut
tertangkap di stream.

Dibangun dengan **Tauri v2 (Rust) + React + TypeScript**. Semua servis jangka panjang
(OBS, chat, alert) berjalan di Rust, sedangkan React hanya menampilkan state.

> Status: **Fase 4, alert masalah stream selesai.** Status OBS, live chat YouTube, dan alert sudah asli.

## Fitur saat ini

- Overlay transparan, tembus klik, tidak mencuri fokus, tersembunyi dari capture
- Widget status OBS & chat yang bisa diseret/diubah ukuran (mode edit)
- **Status OBS asli** lewat obs-websocket v5:
  - LIVE/OFFLINE/RECONNECTING + durasi, bitrate (rata-rata 3 detik), drop % (10 detik terakhir),
    FPS, status rekaman, scene aktif, mic mute
  - tersambung ulang otomatis (1, 2, 4, 8, lalu tiap 10 detik); berhenti mencoba jika password salah
  - kontrol di mode edit: mulai/stop stream (**harus ditahan**), rekam, mute mic, ganti scene
  - notifikasi singkat di overlay untuk hasil hotkey
- **Live chat YouTube tanpa login** (InnerTube, tanpa kuota API):
  - sumber berupa `@handle`, URL channel, atau URL video; untuk channel, live baru terdeteksi
    otomatis (dicek tiap 30 detik)
  - mode *Live chat* (semua pesan, bukan hanya Top chat), polling ±1,5 detik
  - emoji (termasuk emoji kustom channel), badge pemilik/moderator/member/terverifikasi,
    Super Chat & Super Sticker dengan warna tier asli, member baru/milestone, hadiah membership
  - pesan yang dihapus moderator atau dari penonton yang di-ban ikut hilang
  - jumlah penonton (tiap 30 detik) di bar status
- **Alert masalah stream** (widget sendiri, bisa digeser):
  - **kritis** (menetap): stream menyambung ulang, stream berhenti mendadak, koneksi OBS putus
    saat live, frame drop ≥ 5%
  - **peringatan** (10 detik, diulang berkala): frame drop ≥ 1%, bitrate turun di bawah 50% dari
    normal, jaringan padat, CPU OBS tinggi, frame terlewat saat render/encoding
  - **pengingat**: mic mute saat live (tiap 2 menit), chat YouTube bermasalah atau live belum terdeteksi
  - stop yang disengaja (dari aplikasi atau tombol Stop di OBS) tidak dianggap masalah
  - debounce agar lonjakan sesaat tidak memicu alert; semua alert tercatat di log
- Dashboard bertab:
  - **Overlay:** status, pilih monitor, kepekatan latar, ukuran huruf, durasi pudar chat
  - **Koneksi:** status OBS & chat, alamat & password OBS, input mic yang dipantau, sumber chat YouTube
  - **Alert:** alert aktif, ambang (frame drop, bitrate, CPU, pengingat mic), tombol *Tes alert*
  - **Hotkey:** ubah kombinasi tombol (direkam langsung, wajib pakai modifier)
  - **Pengujian:** checklist Fase 4 (alert), Fase 3 (chat), Fase 2 (OBS), dan Fase 0 (overlay)
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
koneksi ke **server OBS tiruan** (password benar/salah/kosong, polling, kontrol, OBS ditutup),
penguraian chat YouTube dari fixture JSON (`src-tauri/tests/fixtures/youtube/`, berisi data
rekaan), pengenalan format sumber chat, serta skenario linimasa mesin alert (debounce, cooldown,
eskalasi, stop disengaja vs mendadak, OBS putus saat live).

### Cek chat YouTube dari terminal

```bash
cd src-tauri && cargo run --example yt_chat -- @namachannel 60
```

Menampilkan judul live, jumlah penonton, riwayat chat, lalu pesan baru selama 60 detik. Berguna
untuk memastikan chat sebuah live bisa dibaca tanpa membuka aplikasi.

### Mencoba tanpa OBS

Server obs-websocket tiruan bisa dijalankan sendiri, mis. untuk melihat overlay dalam keadaan live:

```bash
cd src-tauri && cargo run --example mock_obs -- --live --drop 0.5
```

Opsi: `--port 4455`, `--password RAHASIA`, `--live`, `--drop <persen>`, `--kbps <bitrate>`,
`--mic-muted`. Contoh memicu alert: `--live --drop 3 --mic-muted`, lalu matikan server tiruan untuk
melihat alert "Koneksi ke OBS putus saat live".
Pastikan WebSocket server OBS asli tidak aktif di port yang sama.

## Checklist Fase 4 (alert)

1. **(a) Posisi:** dashboard → Alert → *Tes alert*; atur widget Alert di mode edit agar tidak menutupi HUD game.
2. **(b) Mic:** mute mic saat live → pengingat setelah ±10 detik, diulang tiap 2 menit.
3. **(c) Internet:** batasi upload / matikan Wi-Fi sebentar saat live → alert frame drop atau "menyambung ulang".
4. **(d) Stop disengaja:** stop dari aplikasi → tanpa alert; stop dari OBS → info singkat saja.
5. **(e) OBS ditutup saat live** → alert kritis "Koneksi ke OBS putus saat live".
6. **(f) Log:** setelah stream, setiap alert tercatat di file log beserta jam dan penjelasannya.

## Checklist Fase 3 (chat YouTube)

Checklist yang sama ada di dashboard → Pengujian dan bisa dicentang di sana.

1. **(a) Belum live:** isi @handle channel Anda → status "menunggu live", dicek ulang tiap 30 detik.
2. **(b) Live dimulai:** dalam ±30 detik chat tersambung; judul & jumlah penonton tampil.
   Live *unlisted* tidak muncul di halaman channel, jadi pakai URL videonya.
3. **(c) Cepat:** chat dari HP muncul di overlay ≤ 3 detik; emoji & badge benar.
4. **(d) Moderasi:** pesan yang dihapus / penonton yang di-timeout hilang dari overlay.
5. **(e) Pesan berbayar:** Super Chat, Super Sticker, member baru, dan hadiah membership tampil khusus.
6. **(f) Live selesai:** status kembali "menunggu live"; live berikutnya tersambung otomatis.

## Checklist Fase 2 (OBS)

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
  youtube/        chat YouTube: servis & status (mod.rs), format sumber (source.rs),
                  klien InnerTube (innertube.rs), penguraian JSON (parse.rs), buffer pesan (buffer.rs)
  alerts/         alert: servis 1 Hz (mod.rs), aturan murni + test linimasa (engine.rs)
  settings.rs     struktur & validasi pengaturan, simpan/muat
  accelerator.rs  parser & normalisasi teks hotkey
  hotkeys.rs      hotkey global yang bisa diubah (+ jeda saat direkam)
  secrets.rs      Windows Credential Manager
  commands.rs     command untuk frontend
  lib.rs          setup aplikasi, tray, plugin, log
src-tauri/tests/support/mock_obs.rs   server obs-websocket tiruan (test + example)
src-tauri/examples/mock_obs.rs        `cargo run --example mock_obs`
src-tauri/examples/yt_chat.rs         `cargo run --example yt_chat -- @handle`
src-tauri/tests/fixtures/youtube/     contoh respons InnerTube (data rekaan) untuk test parser
```

## Rencana berikutnya

- ~~**Fase 1:** pengaturan, pemilihan monitor, penyimpanan rahasia~~ selesai
- ~~**Fase 2:** OBS WebSocket (status asli + kontrol)~~ selesai
- ~~**Fase 3:** chat YouTube via InnerTube + jumlah penonton~~ selesai
- ~~**Fase 4:** alert masalah stream~~ selesai
- **Fase 5:** balas chat (OAuth + YouTube Data API)
- **Fase 6:** polishing & installer
