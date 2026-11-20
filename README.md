# LiveStream Overlay (prototype)

Aplikasi desktop Windows untuk streamer satu monitor: **live chat YouTube** dan **status OBS**
tampil sebagai overlay di atas game. Overlay tembus klik, tidak mencuri fokus, dan tidak ikut
tertangkap di stream.

Dibangun dengan **Tauri v2 (Rust) + React + TypeScript**. Semua servis jangka panjang
(OBS, chat, alert) berjalan di Rust, sedangkan React hanya menampilkan state.

> Status: **Fase 8, ringkasan sesi stream + alert "mic tidak terdengar"**. Installer Windows (NSIS) tersedia.
> Yang tersisa: uji langsung saat live sungguhan (checklist di dashboard → Pengujian).

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
  - **mic dari level audio OBS** (Fase 8): "Mic tidak terdengar" jika mic tidak mute tapi tanpa
    suara selama 2 menit (bisa diatur), dan "Suara mic pecah" jika peak menyentuh 0 dB ≥ 3 detik
    dalam 10 detik. Level dibaca dari event `InputVolumeMeters` OBS (20×/detik), hanya saat aturannya aktif
  - stop yang disengaja (dari aplikasi atau tombol Stop di OBS) tidak dianggap masalah
  - debounce agar lonjakan sesaat tidak memicu alert; semua alert tercatat di log
- **Alert donasi Trakteer** (digabung dari trakteer-live-suite):
  - tersambung realtime ke Trakteer: Stream Key V2 (`trv2-…`) atau Channel ID V1 (Pusher)
  - setiap donasi tampil sebagai kartu di widget "Donasi Trakteer" (nama, unit × jumlah,
    nominal, pesan), satu per satu berurutan; badge TEST untuk donasi test dari dashboard Trakteer
  - hanya terlihat oleh streamer (overlay tersembunyi dari capture), tanpa suara
  - filter minimal nominal, tampilkan/sembunyikan donasi test dan pesan, durasi tampil
  - sambung ulang otomatis (1,5 detik ×1,6, maks. 10 detik); pengingat jika terputus saat live
  - kunci disimpan di Windows Credential Manager; log hanya mencatat nominal
- **Ringkasan sesi stream** (Fase 8):
  - dicatat otomatis selama live: durasi, puncak & rata-rata penonton, pesan chat, chatter unik,
    Super Chat, member baru, donasi Trakteer (tanpa test) + top donatur, rata-rata bitrate,
    frame drop, reconnect, dan timeline alert; grafik penonton & bitrate per menit
  - stream putus lalu live lagi dalam 3 menit dihitung satu sesi
  - setelah stream selesai: info "Stream selesai" di overlay dan dashboard terbuka di tab Riwayat
    (bisa dimatikan; sesi < 2 menit tidak membuka dashboard)
  - 30 sesi terakhir disimpan lokal; sesi yang terputus karena aplikasi ditutup dipulihkan
- **Balas chat dari overlay** (opsional, butuh login Google):
  - kotak balas di bawah chat saat mode edit; klik nama penonton untuk menyebut `@nama`
  - login OAuth di browser (PKCE, redirect ke `127.0.0.1`), hanya scope `youtube.force-ssl`
  - pesan dikirim lewat YouTube Data API ke live yang chat-nya sedang tampil
    (50 unit kuota/pesan, ±190 pesan/hari dengan kuota gratis)
  - OAuth client dan refresh token disimpan di Windows Credential Manager
- Dashboard bertab:
  - **Mulai:** panduan awal yang tercentang sendiri saat OBS & chat sudah tersambung
  - **Overlay:** status, pilih monitor, kepekatan latar, ukuran huruf, durasi pudar chat
  - **Koneksi:** status OBS & chat, alamat & password OBS, input mic yang dipantau, sumber chat
    YouTube, kunci Trakteer + riwayat donasi, login Google untuk membalas chat
  - **Alert:** alert aktif, ambang (frame drop, bitrate, CPU, pengingat mic, mic diam, suara
    pecah), meter *Tes mic*, tombol *Tes alert*
  - **Riwayat:** ringkasan sesi terakhir dan sesi-sesi sebelumnya
  - **Hotkey:** ubah kombinasi tombol (direkam langsung, wajib pakai modifier)
  - **Pengujian:** checklist Fase 8 (ringkasan sesi & mic), 7 (Trakteer), 5 (balas chat), 4 (alert), 3 (chat), 2 (OBS), dan 0 (overlay)
- System tray, single-instance, dashboard dihancurkan saat ditutup (hemat RAM)
- Tombol **Buka folder log** di dashboard untuk meninjau riwayat koneksi & alert setelah stream

## Data & log

| Apa | Lokasi |
|---|---|
| Pengaturan | `%APPDATA%\com.livestreamapps.overlay\settings.json` |
| Layout widget | `%APPDATA%\com.livestreamapps.overlay\layout.json` |
| Password OBS | Windows Credential Manager, target `LiveStreamOverlay/obs-websocket` |
| OAuth client Google | Windows Credential Manager, target `LiveStreamOverlay/youtube-oauth-client` |
| Login Google (refresh token) | Windows Credential Manager, target `LiveStreamOverlay/youtube-oauth-token` |
| Kunci Trakteer | Windows Credential Manager, target `LiveStreamOverlay/trakteer-key` |
| Riwayat sesi stream | `%APPDATA%\com.livestreamapps.overlay\sessions.json` (30 sesi terakhir) |
| Log | `%LOCALAPPDATA%\com.livestreamapps.overlay\logs\` |

## Instal (pengguna)

1. Jalankan `LiveStream Overlay_<versi>_x64-setup.exe` (dari `src-tauri/target/release/bundle/nsis/`).
   Instalasi per pengguna, tidak butuh hak administrator. WebView2 sudah bawaan Windows 11.
2. Buka **LiveStream Overlay** dari Start Menu. Dashboard terbuka di tab **Mulai**; ikuti langkahnya.
3. Menutup dashboard tidak mematikan aplikasi: overlay tetap berjalan, buka lagi lewat ikon tray.

Ukuran & beban (build release, dashboard tertutup, tersambung ke OBS): installer 1,8 MB,
RAM ±108 MB (kolom *Memory* Task Manager, termasuk WebView2), CPU idle ±0,04%.

Uninstall lewat *Settings → Apps*. Pengaturan (`%APPDATA%`), log (`%LOCALAPPDATA%`), dan rahasia
di Credential Manager tidak ikut terhapus; hapus manual jika perlu.

## Prasyarat (pengembangan)

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

Hasil: `src-tauri/target/release/bundle/nsis/LiveStream Overlay_<versi>_x64-setup.exe`. Build pertama
mengunduh perangkat NSIS milik Tauri secara otomatis.

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
eskalasi, stop disengaja vs mendadak, OBS putus saat live). Alur login Google (PKCE, redirect,
tukar token, refresh, dibatalkan/ditolak/waktu habis) dan panggilan YouTube Data API diuji
terhadap server HTTP tiruan (`src-tauri/tests/support/mock_http.rs`), tanpa akun sungguhan.
Fase 8 menambah test level mic (`obs/audio.rs`, termasuk langganan & reidentify ke OBS tiruan),
aturan mic diam/pecah, perekam sesi (stop normal, putus lalu lanjut, putus total) dan
penyimpanan riwayat.

### Cek chat YouTube dari terminal

```bash
cd src-tauri && cargo run --example yt_chat -- @namachannel 60
```

Menampilkan judul live, jumlah penonton, riwayat chat, lalu pesan baru selama 60 detik. Berguna
untuk memastikan chat sebuah live bisa dibaca tanpa membuka aplikasi.

### Mencoba alert donasi tanpa donasi asli

Server realtime Trakteer tiruan mengirim donasi rekaan berkala:

```bash
cd src-tauri && cargo run --example mock_trakteer -- --every 8
```

Jalankan aplikasi (build debug) dengan `LSO_TRAKTEER_URL=ws://127.0.0.1:4470`, lalu isi kunci uji
di dashboard → Koneksi → Trakteer, mis. `trv2-uji` (V2) atau `a2FuYWwtdWpp` (V1). Hapus kuncinya
setelah selesai. Tombol **Tes donasi** (tab Overlay) menampilkan contoh kartu tanpa koneksi apa pun.

### Mencoba tanpa OBS

Server obs-websocket tiruan bisa dijalankan sendiri, mis. untuk melihat overlay dalam keadaan live:

```bash
cd src-tauri && cargo run --example mock_obs -- --live --drop 0.5
```

Opsi: `--port 4455`, `--password RAHASIA`, `--live`, `--drop <persen>`, `--kbps <bitrate>`,
`--mic-muted`, `--mic-db <dB>`, `--mic-silent`, `--mic-clip`, `--stop-after <detik>`.
Contoh memicu alert: `--live --drop 3 --mic-muted`, lalu matikan server tiruan untuk
melihat alert "Koneksi ke OBS putus saat live". `--live --mic-silent --stop-after 170` memicu
"Mic tidak terdengar" setelah 2 menit, lalu menghentikan stream sehingga ringkasan sesi muncul.
Pastikan WebSocket server OBS asli tidak aktif di port yang sama.

## Menyiapkan balas chat (sekali saja)

Membaca chat tidak perlu login. Untuk membalas dari overlay, buat OAuth client sendiri:

1. Buka [console.cloud.google.com](https://console.cloud.google.com), buat project baru.
2. **APIs & Services → Library** → aktifkan **YouTube Data API v3**.
3. **OAuth consent screen** (Google Auth Platform) → tipe *External*, isi nama aplikasi & email.
   Di *Audience / Test users*, tambahkan akun Google pemilik channel.
4. **Credentials / Clients → Create OAuth client ID** → tipe **Desktop app**.
5. Salin Client ID & Client secret ke dashboard → Koneksi → *Balas chat*, lalu **Login dengan Google**.

Selama consent screen masih mode *Testing*, login berlaku 7 hari lalu perlu login ulang.

## Checklist Fase 8 (ringkasan sesi & mic)

1. **(a)** Tab Alert → Tes mic: bicara ke mic → bar melewati garis −50 dB.
2. **(b)** Live (tes) dengan device mic salah di OBS → "Mic tidak terdengar" setelah 2 menit.
3. **(c)** Gain mic terlalu tinggi lalu bicara keras → "Suara mic pecah".
4. **(d)** Live ≥ 2 menit lalu stop dari OBS → "Stream selesai" di overlay, dashboard terbuka di tab Riwayat.
5. **(e)** Angka ringkasan dibandingkan dengan YouTube Studio; donasi test Trakteer tidak dihitung.
6. **(f)** Tutup lalu buka aplikasi → sesi masih ada di tab Riwayat; hapus sesi berfungsi.

## Checklist Fase 7 (alert donasi Trakteer)

1. **(a)** Tab Koneksi → Trakteer: isi Stream Key (`trv2-…`) atau Channel ID → "Tersambung".
2. **(b)** "Test alert" di dashboard Trakteer → kartu bertanda TEST muncul di overlay.
3. **(c)** Donasi asli → kartu berurutan; riwayat muncul di tab Koneksi.
4. **(d)** Display Capture di OBS → kartu donasi tidak terlihat di preview.
5. **(e)** Minimal nominal / donasi test dimatikan → donasi di bawah batas tidak tampil.
6. **(f)** Internet putus sebentar → "mencoba ulang", lalu tersambung lagi otomatis.

## Checklist Fase 5 (balas chat)

1. **(a)** OAuth client tersimpan.
2. **(b)** Login Google berhasil → "Login sebagai <channel>".
3. **(c)** Saat live: `Ctrl+Shift+E`, ketik di kotak balas, Enter → pesan muncul di chat YouTube.
4. **(d)** Klik nama penonton → kotak balas terisi `@nama`.
5. **(e)** Tutup & buka aplikasi → masih login.
6. **(f)** Logout → kotak balas meminta login lagi; izin dicabut dari akun Google.

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
                  bitrate & drop % (stats.rs), level mic dari volume meter (audio.rs)
  youtube/        chat YouTube: servis & status (mod.rs), format sumber (source.rs),
                  klien InnerTube (innertube.rs), penguraian JSON (parse.rs), buffer pesan (buffer.rs),
                  login Google (oauth.rs), YouTube Data API (api.rs), akun & kirim pesan (account.rs)
  alerts/         alert: servis 1 Hz (mod.rs), aturan murni + test linimasa (engine.rs)
  trakteer/       alert donasi: koneksi V1/V2 + reconnect (mod.rs), parser protokol murni (protocol.rs)
  session/        ringkasan sesi: servis 1 Hz + hook (mod.rs), perekam murni (recorder.rs),
                  riwayat sessions.json (store.rs)
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
src-tauri/tests/support/mock_http.rs  server HTTP tiruan (Google OAuth & Data API) untuk test
src-tauri/tests/support/mock_trakteer.rs + examples/mock_trakteer.rs  server Trakteer tiruan
```

## Rencana berikutnya

- ~~**Fase 1:** pengaturan, pemilihan monitor, penyimpanan rahasia~~ selesai
- ~~**Fase 2:** OBS WebSocket (status asli + kontrol)~~ selesai
- ~~**Fase 3:** chat YouTube via InnerTube + jumlah penonton~~ selesai
- ~~**Fase 4:** alert masalah stream~~ selesai
- ~~**Fase 5:** balas chat (OAuth + YouTube Data API)~~ selesai
- ~~**Fase 6:** polishing & installer~~ selesai (uji live sungguhan menunggu user)
- ~~**Fase 7:** alert donasi Trakteer (gabung dari trakteer-live-suite)~~ selesai
- ~~**Fase 8:** ringkasan sesi stream + alert mic tidak terdengar / pecah~~ selesai
- **Berikutnya:** command engine donasi → keyboard/mouse dari trakteer-live-suite
