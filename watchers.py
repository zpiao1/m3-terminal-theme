"""Win32 change notifications, so colour changes wake the sync loop instantly
instead of waiting for the next poll.

RegistryWatcher uses RegNotifyChangeKeyValue - the same mechanism Chrome uses
to follow the accent colour. DirectoryWatcher uses FindFirstChangeNotification.
Both block a background thread in WaitForMultipleObjects on two handles: the
notification, and a stop event, so stop() interrupts the wait immediately.

A notification only means "something under this key/folder changed" - the
DWM key holds several values and the Themes folder several files - so callers
still compare a change token before acting.
"""
import ctypes
import threading
import winreg
from ctypes import wintypes

_advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
_kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

HANDLE = wintypes.HANDLE

_kernel32.CreateEventW.restype = HANDLE
_kernel32.CreateEventW.argtypes = [ctypes.c_void_p, wintypes.BOOL, wintypes.BOOL,
                                   wintypes.LPCWSTR]
_kernel32.SetEvent.argtypes = [HANDLE]
_kernel32.CloseHandle.argtypes = [HANDLE]
_kernel32.WaitForMultipleObjects.restype = wintypes.DWORD
_kernel32.WaitForMultipleObjects.argtypes = [wintypes.DWORD, ctypes.POINTER(HANDLE),
                                             wintypes.BOOL, wintypes.DWORD]
_kernel32.FindFirstChangeNotificationW.restype = HANDLE
_kernel32.FindFirstChangeNotificationW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL,
                                                   wintypes.DWORD]
_kernel32.FindNextChangeNotification.argtypes = [HANDLE]
_kernel32.FindCloseChangeNotification.argtypes = [HANDLE]
_advapi32.RegNotifyChangeKeyValue.restype = wintypes.LONG
_advapi32.RegNotifyChangeKeyValue.argtypes = [HANDLE, wintypes.BOOL, wintypes.DWORD,
                                              HANDLE, wintypes.BOOL]

INFINITE = 0xFFFFFFFF
WAIT_OBJECT_0 = 0
INVALID_HANDLE_VALUE = HANDLE(-1).value
REG_NOTIFY_CHANGE_LAST_SET = 0x00000004
FILE_NOTIFY_CHANGE_FILE_NAME = 0x00000001   # Windows may replace the file
FILE_NOTIFY_CHANGE_LAST_WRITE = 0x00000010  # ...or rewrite it in place


class _Watcher(threading.Thread):
    def __init__(self, on_change):
        super().__init__(daemon=True)
        self.on_change = on_change
        self._stop_handle = _kernel32.CreateEventW(None, True, False, None)
        self._armed = threading.Event()
        self._error = None

    # subclasses implement: _open() -> handle, _rearm(handle), _close(handle)

    def start(self):
        super().start()
        # Don't return until the notification is registered; a change made
        # between start() and arming would otherwise be missed.
        self._armed.wait(2.0)
        if self._error:
            raise self._error

    def run(self):
        try:
            handle = self._open()
        except OSError as exc:
            self._error = exc
            self._armed.set()
            return
        self._armed.set()
        handles = (HANDLE * 2)(handle, self._stop_handle)
        try:
            while True:
                r = _kernel32.WaitForMultipleObjects(2, handles, False, INFINITE)
                if r != WAIT_OBJECT_0:
                    break                           # stop requested, or error
                # Re-arm BEFORE the callback so a change made while it runs
                # is not lost.
                self._rearm(handle)
                try:
                    self.on_change()
                except Exception:
                    pass                            # never kill the watcher
        finally:
            self._close(handle)

    def stop(self):
        _kernel32.SetEvent(self._stop_handle)
        if self.is_alive():
            self.join(2.0)
        _kernel32.CloseHandle(self._stop_handle)


class RegistryWatcher(_Watcher):
    """Calls on_change() whenever a value directly under the subkey of root is set."""

    def __init__(self, root, subkey, on_change):
        super().__init__(on_change)
        self.root, self.subkey = root, subkey

    def _open(self):
        self._key = winreg.OpenKey(self.root, self.subkey, 0, winreg.KEY_NOTIFY)
        event = _kernel32.CreateEventW(None, False, False, None)
        self._rearm(event)
        return event

    def _rearm(self, event):
        # Registered from this thread; the watcher thread stays alive for the
        # whole lifetime of the registration, as the API requires.
        rc = _advapi32.RegNotifyChangeKeyValue(self._key.handle, False,
                                               REG_NOTIFY_CHANGE_LAST_SET,
                                               event, True)
        if rc != 0:
            raise OSError(rc, "RegNotifyChangeKeyValue failed")

    def _close(self, event):
        _kernel32.CloseHandle(event)
        self._key.Close()


class DirectoryWatcher(_Watcher):
    """Calls on_change() whenever a file in `path` is written or replaced."""

    def __init__(self, path, on_change):
        super().__init__(on_change)
        self.path = path

    def _open(self):
        handle = _kernel32.FindFirstChangeNotificationW(
            self.path, False,
            FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME)
        if handle in (None, INVALID_HANDLE_VALUE):
            raise ctypes.WinError(ctypes.get_last_error())
        return handle

    def _rearm(self, handle):
        _kernel32.FindNextChangeNotification(handle)

    def _close(self, handle):
        _kernel32.FindCloseChangeNotification(handle)
