import { useState } from "react";

export interface ChecklistItem {
  id: string;
  title: string;
  how: string;
}

export const PHASE0_ITEMS: ChecklistItem[] = [
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
];

export const PHASE7_ITEMS: ChecklistItem[] = [
  {
    id: "a",
    title: "Kunci Trakteer tersimpan & tersambung",
    how: "Tab Koneksi → Trakteer: isi Stream Key (trv2-…) atau Channel ID. Status berubah \"Tersambung\".",
  },
  {
    id: "b",
    title: "Test alert dari Trakteer",
    how: "Tekan \"Test alert\" di dashboard Trakteer: kartu bertanda TEST muncul di overlay.",
  },
  {
    id: "c",
    title: "Donasi asli tampil",
    how: "Donasi masuk: kartu muncul berurutan (nama, nominal, pesan). Riwayat muncul di tab Koneksi.",
  },
  {
    id: "d",
    title: "Tidak terlihat di stream",
    how: "Dengan Display Capture di OBS, kartu donasi tidak terlihat di preview.",
  },
  {
    id: "e",
    title: "Filter",
    how: "Atur minimal nominal / matikan donasi test di tab Overlay: donasi di bawah batas tidak tampil.",
  },
  {
    id: "f",
    title: "Pulih saat internet putus",
    how: "Putus internet sebentar: status \"mencoba ulang\", lalu tersambung lagi otomatis.",
  },
];

export const PHASE5_ITEMS: ChecklistItem[] = [
  {
    id: "a",
    title: "OAuth client tersimpan",
    how: "Ikuti langkah di tab Koneksi → Balas chat, lalu simpan Client ID & secret.",
  },
  {
    id: "b",
    title: "Login Google berhasil",
    how: "Klik Login dengan Google, pilih akun channel Anda, izinkan. Status berubah \"Login sebagai …\".",
  },
  {
    id: "c",
    title: "Balas dari overlay",
    how: "Saat live: Ctrl+Shift+E, ketik di kotak balas bawah chat, Enter. Pesan muncul di chat YouTube.",
  },
  {
    id: "d",
    title: "Sebut penonton",
    how: "Di mode edit, klik nama penonton: kotak balas terisi \"@nama \".",
  },
  {
    id: "e",
    title: "Tetap login setelah aplikasi dibuka ulang",
    how: "Tutup lalu buka aplikasi: masih login dan bisa langsung membalas.",
  },
  {
    id: "f",
    title: "Logout",
    how: "Klik Logout: kotak balas meminta login lagi; izin aplikasi dicabut di akun Google.",
  },
];

export const PHASE4_ITEMS: ChecklistItem[] = [
  {
    id: "a",
    title: "Posisi & tampilan alert",
    how: "Dashboard → Alert → Tes alert. Atur posisi widget Alert di mode edit agar tidak menutupi HUD game.",
  },
  {
    id: "b",
    title: "Mic mute saat live",
    how: "Mute mic selama live: pengingat muncul setelah ±10 detik dan diulang tiap 2 menit.",
  },
  {
    id: "c",
    title: "Internet bermasalah",
    how: "Batasi upload / matikan Wi-Fi sebentar saat live: muncul alert frame drop atau \"OBS menyambung ulang\".",
  },
  {
    id: "d",
    title: "Stop disengaja tidak dianggap masalah",
    how: "Stop stream dari aplikasi: tanpa alert. Stop dari OBS: hanya info singkat.",
  },
  {
    id: "e",
    title: "OBS ditutup saat live",
    how: "Tutup OBS ketika live (mis. pakai server OBS tiruan): alert kritis \"Koneksi ke OBS putus\".",
  },
  {
    id: "f",
    title: "Riwayat di log",
    how: "Setelah stream, buka file log: setiap alert tercatat dengan jam dan penjelasannya.",
  },
];

export const PHASE3_ITEMS: ChecklistItem[] = [
  {
    id: "a",
    title: "Channel belum live terdeteksi",
    how: "Isi @handle channel Anda saat belum live: status \"menunggu live\" dan dicek ulang tiap 30 detik.",
  },
  {
    id: "b",
    title: "Tersambung saat live dimulai",
    how: "Mulai live publik: dalam ±30 detik chat tersambung, judul & jumlah penonton tampil. Live unlisted: pakai URL video.",
  },
  {
    id: "c",
    title: "Pesan muncul ≤ 3 detik",
    how: "Kirim chat dari HP. Pesan, emoji, dan badge (moderator/member) tampil benar di overlay.",
  },
  {
    id: "d",
    title: "Pesan yang dihapus hilang",
    how: "Hapus pesan atau timeout penonton dari YouTube Studio: pesannya hilang dari overlay.",
  },
  {
    id: "e",
    title: "Super Chat / member (jika ada)",
    how: "Super Chat, Super Sticker, member baru, dan hadiah membership tampil dengan warna/ikon khusus.",
  },
  {
    id: "f",
    title: "Live selesai → menunggu live berikutnya",
    how: "Akhiri live: status berubah \"menunggu live\". Live berikutnya tersambung otomatis.",
  },
];

export const PHASE2_ITEMS: ChecklistItem[] = [
  {
    id: "a",
    title: "Terhubung ke OBS",
    how: "Aktifkan WebSocket server di OBS, isi password di tab Koneksi. Status harus \"Terhubung\".",
  },
  {
    id: "b",
    title: "Password salah ditolak dengan jelas",
    how: "Isi password yang salah: muncul \"Password OBS salah\" dan aplikasi berhenti mencoba. Isi yang benar → terhubung lagi.",
  },
  {
    id: "c",
    title: "Angka sama dengan OBS",
    how: "Mulai stream unlisted. Bitrate, drop %, dan FPS di overlay mendekati dock Stats OBS (selisih ±5%).",
  },
  {
    id: "d",
    title: "Pulih sendiri saat OBS ditutup",
    how: "Tutup OBS: widget menampilkan \"OBS belum terhubung\". Buka lagi: tersambung otomatis dalam ±10 detik.",
  },
  {
    id: "e",
    title: "Hotkey mic & rekaman di dalam game",
    how: "Tekan Ctrl+Shift+M dan Ctrl+Shift+R saat game fokus. Ikon mic/REC berubah dan muncul notifikasi singkat.",
  },
  {
    id: "f",
    title: "Kontrol di mode edit",
    how: "Ctrl+Shift+E: ganti scene, mute mic, rekam. Stop stream hanya jalan jika tombolnya ditahan 1,5 detik.",
  },
];

type Checked = Record<string, boolean>;

function readChecked(key: string): Checked {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as Checked) : {};
  } catch {
    return {};
  }
}

function writeChecked(key: string, value: Checked) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Penyimpanan tidak tersedia; checklist tetap berfungsi selama sesi ini.
  }
}

/** Checklist uji manual; centang disimpan per browser (localStorage). */
export function Checklist({
  storageKey,
  items,
  doneText,
}: {
  storageKey: string;
  items: ChecklistItem[];
  doneText: string;
}) {
  const [checked, setChecked] = useState<Checked>(() => readChecked(storageKey));

  const toggle = (id: string) => {
    setChecked((prev) => {
      const next = { ...prev, [id]: !prev[id] };
      writeChecked(storageKey, next);
      return next;
    });
  };

  const done = items.filter((i) => checked[i.id]).length;

  return (
    <>
      <p className="muted">
        {done} dari {items.length} lulus.
        {done === items.length && ` ${doneText}`}
      </p>
      <ul className="checklist">
        {items.map((item) => (
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
