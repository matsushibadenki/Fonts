//! Register persistent, per-user fonts. No administrator or Adobe cache writes.
use std::path::Path;
#[cfg(target_os = "macos")]
pub fn change(path: &Path, enable: bool) -> Result<(), String> {
    use std::{ffi::c_void, os::unix::ffi::OsStrExt};
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            a: *const c_void,
            b: *const u8,
            len: isize,
            is_dir: u8,
        ) -> *const c_void;
        fn CFRelease(value: *const c_void);
    }
    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontManagerRegisterFontsForURL(
            url: *const c_void,
            scope: u32,
            error: *mut *const c_void,
        ) -> u8;
        fn CTFontManagerUnregisterFontsForURL(
            url: *const c_void,
            scope: u32,
            error: *mut *const c_void,
        ) -> u8;
    }
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: immutable path bytes outlive URL creation; all owned CF values are released.
    unsafe {
        let url = CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            bytes.as_ptr(),
            bytes.len() as isize,
            0,
        );
        if url.is_null() {
            return Err("font_registration_failed".into());
        }
        let mut error = std::ptr::null();
        let ok = if enable {
            CTFontManagerRegisterFontsForURL(url, 2, &mut error)
        } else {
            CTFontManagerUnregisterFontsForURL(url, 2, &mut error)
        };
        CFRelease(url);
        if !error.is_null() {
            CFRelease(error);
        }
        if ok == 0 {
            return Err("font_registration_failed".into());
        }
    }
    Ok(())
}
#[cfg(target_os = "windows")]
pub fn change(path: &Path, enable: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Graphics::Gdi::{AddFontResourceExW, RemoveFontResourceExW},
        UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_FONTCHANGE,
        },
    };
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(r"Software\Microsoft\Windows NT\CurrentVersion\Fonts")
        .map_err(|e| e.to_string())?
        .0;
    let value = path
        .file_stem()
        .ok_or("font_invalid_path")?
        .to_string_lossy()
        .to_string()
        + " (TrueType)";
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: NUL-terminated path and no reserved data. Broadcast is bounded to avoid hanging on other apps.
    unsafe {
        if enable {
            key.set_value(&value, &path.to_string_lossy().to_string())
                .map_err(|e| e.to_string())?;
            if AddFontResourceExW(wide.as_ptr(), 0, std::ptr::null()) == 0 {
                let _ = key.delete_value(&value);
                return Err("font_registration_failed".into());
            }
        } else {
            if RemoveFontResourceExW(wide.as_ptr(), 0, std::ptr::null()) == 0 {
                return Err("font_registration_failed".into());
            }
            if let Err(e) = key.delete_value(&value) {
                AddFontResourceExW(wide.as_ptr(), 0, std::ptr::null());
                return Err(e.to_string());
            }
        }
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_FONTCHANGE,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            std::ptr::null_mut(),
        );
    }
    Ok(())
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn change(path: &Path, _enable: bool) -> Result<(), String> {
    let directory = path.parent().ok_or("font_invalid_path")?;
    let status = std::process::Command::new("fc-cache")
        .arg("-f")
        .arg(directory)
        .status()
        .map_err(|_| "font_fontconfig_unavailable")?;
    if status.success() {
        Ok(())
    } else {
        Err("font_registration_failed".into())
    }
}
