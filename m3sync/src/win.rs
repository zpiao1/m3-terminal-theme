//! The Win32 calls m3sync needs: registry values, change notifications and
//! the WM_SETTINGCHANGE broadcast.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, REG_SZ, RRF_RT_DWORD, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_SZ, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};

pub const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Apps light/dark mode; dark if the value can't be read.
pub fn is_dark() -> bool {
    let mut value = 0u32;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(PERSONALIZE_KEY).as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            RRF_RT_DWORD,
            std::ptr::null_mut(),
            (&raw mut value).cast(),
            &mut size,
        )
    };
    rc != ERROR_SUCCESS || value == 0
}

pub fn get_env_prompt() -> Option<String> {
    let (key, name) = (wide("Environment"), wide("PROMPT"));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | 0x1000_0000; // RRF_NOEXPAND
    let mut size = 0u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), std::ptr::null_mut(), &mut size)
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub fn set_env_prompt(text: &str) -> Result<(), String> {
    let data = wide(text);
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            wide("Environment").as_ptr(),
            wide("PROMPT").as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    if rc == ERROR_SUCCESS { Ok(()) } else { Err(format!("RegSetKeyValueW failed ({rc})")) }
}

/// Tell Explorer (and so new terminals/tabs) that the environment changed.
pub fn broadcast_environment_change() {
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            wide("Environment").as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        );
    }
}

/// Built for the windows subsystem (no console flashes at login); when run
/// from a terminal, write --preview / --once output to that terminal.
pub fn attach_parent_console() {
    unsafe {
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null() {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

// --- Change notifications --------------------------------------------------
//
// Each watcher blocks a thread on one notification handle and sends a unit
// on the channel per change. A notification only means "something under this
// key/folder changed" - the Personalize key holds several values and the
// Themes folder several files - so the receiver compares what it cares about.

/// Sends whenever a value directly under HKCU\`subkey` is set.
pub fn watch_registry(subkey: &str, tx: Sender<()>) -> Result<(), String> {
    let mut key: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), 0, KEY_NOTIFY, &mut key) };
    if rc != ERROR_SUCCESS {
        return Err(format!("RegOpenKeyExW({subkey}) failed ({rc})"));
    }
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    let (key, event) = (key as usize, event as usize); // raw handles aren't Send
    // Registered (and re-registered after each signal) from the watcher
    // thread, which lives as long as the registration, as the API requires.
    let arm = move || unsafe {
        RegNotifyChangeKeyValue(key as HKEY, 0, REG_NOTIFY_CHANGE_LAST_SET, event as HANDLE, 1) == ERROR_SUCCESS
    };
    spawn_watcher(event, arm, arm, tx)
}

/// Sends whenever a file in `dir` is written or replaced.
pub fn watch_directory(dir: &Path, tx: Sender<()>) -> Result<(), String> {
    // Windows may replace the file, or rewrite it in place.
    let filter = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE;
    let handle = unsafe { FindFirstChangeNotificationW(wide(dir).as_ptr(), 0, filter) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("FindFirstChangeNotificationW({}) failed: {}", dir.display(), std::io::Error::last_os_error()));
    }
    let handle = handle as usize;
    spawn_watcher(handle, || true, move || unsafe { FindNextChangeNotification(handle as HANDLE) != 0 }, tx)
}

/// Waits on `handle` (signalled per change). `arm` registers the notification
/// on the watcher thread; `rearm` runs after each signal, BEFORE sending, so a
/// change made while the receiver works is not lost.
fn spawn_watcher(
    handle: usize,
    arm: impl FnOnce() -> bool + Send + 'static,
    rearm: impl Fn() -> bool + Send + 'static,
    tx: Sender<()>,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let armed = arm();
        let _ = ready_tx.send(armed);
        if !armed {
            return;
        }
        while unsafe { WaitForSingleObject(handle as HANDLE, INFINITE) } == WAIT_OBJECT_0 {
            if !rearm() || tx.send(()).is_err() {
                break;
            }
        }
        unsafe { CloseHandle(handle as HANDLE) };
    });
    // Don't return until the notification is registered; a change made
    // between spawning and arming would otherwise be missed.
    match ready_rx.recv() {
        Ok(true) => Ok(()),
        _ => Err("could not register the change notification".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use super::*;

    const KEY: &str = r"Software\m3sync-watcher-test";

    fn set(value: u32) {
        let data = value.to_le_bytes();
        let rc = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide("v").as_ptr(), 4 /* REG_DWORD */, data.as_ptr().cast(), 4)
        };
        assert_eq!(rc, ERROR_SUCCESS);
    }

    #[test]
    fn registry_watcher_fires_on_every_change() {
        set(0);
        let (tx, rx) = channel();
        watch_registry(KEY, tx).unwrap();
        for i in 1..=3 {
            set(i); // each change must re-arm, not just the first
            rx.recv_timeout(Duration::from_secs(2)).unwrap_or_else(|_| panic!("no notification for change {i}"));
            while rx.try_recv().is_ok() {}
        }
        unsafe { windows_sys::Win32::System::Registry::RegDeleteTreeW(HKEY_CURRENT_USER, wide(KEY).as_ptr()) };
    }

    #[test]
    fn directory_watcher_fires_on_write_and_replace() {
        let dir = std::env::temp_dir().join(format!("m3sync-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = channel();
        watch_directory(&dir, tx).unwrap();
        std::fs::write(dir.join("a"), "1").unwrap();
        rx.recv_timeout(Duration::from_secs(2)).expect("no notification for write");
        while rx.try_recv().is_ok() {}
        std::fs::write(dir.join("b.tmp"), "2").unwrap();
        std::fs::rename(dir.join("b.tmp"), dir.join("a")).unwrap();
        rx.recv_timeout(Duration::from_secs(2)).expect("no notification for replace");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
