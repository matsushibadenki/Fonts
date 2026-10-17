//! Versioned JSON projects with embedded image assets; shared by manual save and recovery.
use crate::{Error, FontDocument, Result};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub fn load(path: &Path) -> Result<FontDocument> {
    let file = fs::File::open(path).map_err(|_| Error::Io)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    let doc: FontDocument = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidFile)?;
    doc.validate()?;
    Ok(doc)
}

/// Same-directory temporary file + sync + atomic rename. On error the previous
/// document remains intact. Parent directory fsync/power-loss durability is not promised.
pub fn save(path: &Path, document: &FontDocument) -> Result<()> {
    document.validate()?;
    let bytes = serde_json::to_vec(document).map_err(|_| Error::InvalidFile)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    write_bytes(path, &bytes)
}

/// Atomic replacement for project, font, and manifest output.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or(Error::Io)?;
    let (temporary, mut file) = (0..64)
        .find_map(|_| {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let mut temp_name = name.to_os_string();
            temp_name.push(format!(".{}.{}.tmp", std::process::id(), id));
            let temporary = parent.join(temp_name);
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .ok()
                .map(|f| (temporary, f))
        })
        .ok_or(Error::Io)?;
    let result = (|| {
        file.write_all(bytes).map_err(|_| Error::Io)?;
        file.sync_all().map_err(|_| Error::Io)?;
        drop(file);
        atomic_replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).map_err(|_| Error::Io)
}
#[cfg(target_os = "windows")]
fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both buffers are NUL-terminated and remain valid for this synchronous call.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
        Err(Error::Io)
    } else {
        Ok(())
    }
}
