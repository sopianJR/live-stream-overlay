//! Penyimpanan rahasia di Windows Credential Manager (terenkripsi per akun Windows).
//!
//! Password tidak pernah dikirim balik ke frontend; frontend hanya tahu apakah
//! password sudah tersimpan atau belum.

const OBS_PASSWORD_TARGET: &str = "LiveStreamOverlay/obs-websocket";
/// Batas ukuran blob Credential Manager (CRED_MAX_CREDENTIAL_BLOB_SIZE).
const MAX_SECRET_BYTES: usize = 5 * 512;

pub fn set_obs_password(password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err("Password kosong. Gunakan tombol Hapus untuk menghapus password.".into());
    }
    if password.len() > MAX_SECRET_BYTES {
        return Err("Password terlalu panjang.".into());
    }
    imp::write(OBS_PASSWORD_TARGET, password)
}

/// Dipakai koneksi OBS (Fase 2).
#[allow(dead_code)]
pub fn obs_password() -> Result<Option<String>, String> {
    imp::read(OBS_PASSWORD_TARGET)
}

pub fn has_obs_password() -> Result<bool, String> {
    imp::read(OBS_PASSWORD_TARGET).map(|p| p.is_some())
}

pub fn clear_obs_password() -> Result<(), String> {
    imp::delete(OBS_PASSWORD_TARGET)
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND, FILETIME};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn write(target: &str, secret: &str) -> Result<(), String> {
        let mut target_w = wide(target);
        let mut user_w = wide("livestream-overlay");
        let blob = secret.as_bytes();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target_w.as_mut_ptr(),
            Comment: std::ptr::null_mut(),
            LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: std::ptr::null_mut(),
            UserName: user_w.as_mut_ptr(),
        };
        // SAFETY: semua pointer menunjuk buffer yang hidup selama pemanggilan.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(format!("Gagal menyimpan ke Credential Manager (kode {}).", unsafe {
                GetLastError()
            }));
        }
        Ok(())
    }

    pub fn read(target: &str) -> Result<Option<String>, String> {
        let target_w = wide(target);
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `credential` diisi Windows dan dibebaskan dengan CredFree di bawah.
        if unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            let code = unsafe { GetLastError() };
            return if code == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(format!("Gagal membaca Credential Manager (kode {code})."))
            };
        }
        let secret = unsafe {
            let c = &*credential;
            let text = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
                String::new()
            } else {
                let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
                String::from_utf8_lossy(bytes).into_owned()
            };
            CredFree(credential as *const _);
            text
        };
        Ok(Some(secret))
    }

    pub fn delete(target: &str) -> Result<(), String> {
        let target_w = wide(target);
        if unsafe { CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            let code = unsafe { GetLastError() };
            if code != ERROR_NOT_FOUND {
                return Err(format!("Gagal menghapus dari Credential Manager (kode {code})."));
            }
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_password_without_touching_store() {
        assert!(set_obs_password("").unwrap_err().contains("kosong"));
    }

    /// Menulis ke Credential Manager asli dengan target khusus test, lalu membersihkannya.
    #[test]
    fn credential_roundtrip() {
        let target = format!("LiveStreamOverlay/test-{}", std::process::id());
        imp::delete(&target).unwrap();
        assert_eq!(imp::read(&target).unwrap(), None);

        imp::write(&target, "rahasia-ü-123").unwrap();
        assert_eq!(imp::read(&target).unwrap().as_deref(), Some("rahasia-ü-123"));

        imp::write(&target, "ganti").unwrap();
        assert_eq!(imp::read(&target).unwrap().as_deref(), Some("ganti"));

        imp::delete(&target).unwrap();
        assert_eq!(imp::read(&target).unwrap(), None);
        imp::delete(&target).unwrap(); // menghapus yang tidak ada bukan error
    }
}

#[cfg(not(windows))]
mod imp {
    const UNSUPPORTED: &str = "Penyimpanan rahasia hanya didukung di Windows.";

    pub fn write(_target: &str, _secret: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub fn read(_target: &str) -> Result<Option<String>, String> {
        Err(UNSUPPORTED.into())
    }
    pub fn delete(_target: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}
