//! The Win32 calls m3sync needs: registry values, change notifications and
//! the WM_SETTINGCHANGE broadcast.
//!
//! The only module allowed `unsafe` (main.rs denies it everywhere else).
//! Every block is a direct Win32 call; its SAFETY comment says why the
//! pointers and handles it passes are valid for the duration of the call.
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FindCloseChangeNotification, FindFirstChangeNotificationW,
    FindNextChangeNotification,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, REG_SZ, RRF_NOEXPAND, RRF_RT_DWORD, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};

pub const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const ENVIRONMENT_KEY: &str = "Environment";
const PROMPT_VALUE: &str = "PROMPT";

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Apps light/dark mode; dark if the value can't be read.
pub fn is_dark() -> bool {
    let mut value = 0u32;
    let mut size = 4u32;
    // SAFETY: both strings are NUL-terminated and outlive the call; `value`
    // is a u32 and `size` says so, so the API writes at most 4 bytes.
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
    let (key, name) = (wide(ENVIRONMENT_KEY), wide(PROMPT_VALUE));
    // SAFETY: key and name are NUL-terminated and outlive the closure; `buf`
    // is null (size query) or points to a buffer of at least `*size` bytes.
    let query = |buf: *mut u16, size: &mut u32| unsafe {
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), buf.cast(), size)
    };
    let mut size = 0u32;
    if query(std::ptr::null_mut(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    if query(buf.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub fn set_env_prompt(text: &str) -> Result<(), String> {
    let data = wide(text);
    // SAFETY: key and value names are NUL-terminated temporaries that live
    // until the end of the statement; `data` holds exactly the length passed.
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            wide(ENVIRONMENT_KEY).as_ptr(),
            wide(PROMPT_VALUE).as_ptr(),
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
    // SAFETY: lParam is a NUL-terminated string that lives until the end of
    // the statement, and SMTO_ABORTIFHUNG with a timeout bounds the wait.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            wide(ENVIRONMENT_KEY).as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        );
    }
}

/// Built for the windows subsystem (no console flashes at login); when run
/// from a terminal, write --preview / --once output to that terminal.
pub fn attach_parent_console() {
    // SAFETY: neither call takes pointers; both are harmless if there is no
    // parent console (AttachConsole just fails).
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
    // SAFETY: subkey is NUL-terminated and outlives the call; `key` is a
    // valid out-pointer.
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), 0, KEY_NOTIFY, &mut key) };
    if rc != ERROR_SUCCESS {
        return Err(format!("RegOpenKeyExW({subkey}) failed ({rc})"));
    }
    // SAFETY: null security attributes and name are allowed: an unnamed
    // auto-reset event.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if event.is_null() {
        let err = std::io::Error::last_os_error();
        // SAFETY: `key` was opened above and is not used after this.
        unsafe { RegCloseKey(key) };
        return Err(format!("CreateEventW failed: {err}"));
    }
    let (key, event) = (key as usize, event as usize); // raw handles aren't Send
    // Registered (and re-registered after each signal) from the watcher
    // thread, which lives as long as the registration, as the API requires.
    // SAFETY: the watcher thread owns `key` and `event` and closes them only
    // after its last call to this.
    let arm = move || unsafe {
        RegNotifyChangeKeyValue(key as HKEY, 0, REG_NOTIFY_CHANGE_LAST_SET, event as HANDLE, 1) == ERROR_SUCCESS
    };
    // SAFETY: runs once, on the watcher thread, after its last use of either handle.
    let close = move || unsafe {
        CloseHandle(event as HANDLE);
        RegCloseKey(key as HKEY);
    };
    spawn_watcher(event, arm, arm, close, tx)
}

/// Sends whenever a file in `dir` is written or replaced.
pub fn watch_directory(dir: &Path, tx: Sender<()>) -> Result<(), String> {
    // Windows may replace the file, or rewrite it in place.
    let filter = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE;
    // SAFETY: the path is a NUL-terminated temporary that outlives the call.
    let handle = unsafe { FindFirstChangeNotificationW(wide(dir).as_ptr(), 0, filter) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("FindFirstChangeNotificationW({}) failed: {}", dir.display(), std::io::Error::last_os_error()));
    }
    let handle = handle as usize;
    // SAFETY: the watcher thread owns `handle` and calls this before `close`.
    let rearm = move || unsafe { FindNextChangeNotification(handle as HANDLE) != 0 };
    // SAFETY: runs once, on the watcher thread, after its last use of `handle`.
    let close = move || unsafe {
        FindCloseChangeNotification(handle as HANDLE);
    };
    spawn_watcher(handle, || true, rearm, close, tx)
}

/// Waits on `handle` (signalled per change). `arm` registers the notification
/// on the watcher thread; `rearm` runs after each signal, BEFORE sending, so a
/// change made while the receiver works is not lost. `close` releases the
/// handles when the thread ends, including when arming fails.
fn spawn_watcher(
    handle: usize,
    arm: impl FnOnce() -> bool + Send + 'static,
    rearm: impl Fn() -> bool + Send + 'static,
    close: impl FnOnce() + Send + 'static,
    tx: Sender<()>,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let armed = arm();
        let _ = ready_tx.send(armed);
        // SAFETY: `handle` stays open until `close` below.
        while armed && unsafe { WaitForSingleObject(handle as HANDLE, INFINITE) } == WAIT_OBJECT_0 {
            if !rearm() || tx.send(()).is_err() {
                break;
            }
        }
        close();
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
        // SAFETY: NUL-terminated names; `data` is the 4 bytes passed.
        let rc = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide("v").as_ptr(), windows_sys::Win32::System::Registry::REG_DWORD, data.as_ptr().cast(), 4)
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
        // SAFETY: NUL-terminated temporary that outlives the call.
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
