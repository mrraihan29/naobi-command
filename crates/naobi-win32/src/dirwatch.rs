//! Directory watch for the index worker (REPORT 3.5): one overlapped `ReadDirectoryChangesW` per
//! directory, waited on by the thread that owns the watch. It has no thread of its own and no timer
//! (review minor M12), so an idle app wakes for nothing.
use std::ffi::c_void;
use std::path::Path;
use std::time::Duration;

use windows::core::{Error, Result, HSTRING};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
    FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{
    CreateEventW, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

/// Room for the change records: 16 KiB. An overflow reports as a change with no records.
const BUF_WORDS: usize = 4096;

/// What `DirWatcher::wait` saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    Changed,
    Timeout,
}

/// The read in flight. Boxed, so the kernel writes into memory that does not move.
struct Io {
    ov: OVERLAPPED,
    bytes: u32,
    buf: [u32; BUF_WORDS],
}

/// One watched directory. The handles are closed on drop, after the pending read has ended.
pub struct DirWatcher {
    dir: HANDLE,
    event: HANDLE,
    io: Box<Io>,
    /// Also report changes in the folders below `dir`.
    subtree: bool,
}

// SAFETY: the handles and the boxed read belong to this value alone and are touched only by the thread
// that holds it; the watcher is moved to its thread once, before any call.
unsafe impl Send for DirWatcher {}

impl DirWatcher {
    /// Opens `dir` and starts one read for creations, renames, writes and size changes.
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_with(dir, false)
    }

    /// Like `open`, and the folders below `dir` are watched too (the Start Menu has sub-folders).
    pub fn open_subtree(dir: &Path) -> Result<Self> {
        Self::open_with(dir, true)
    }

    fn open_with(dir: &Path, subtree: bool) -> Result<Self> {
        let name = HSTRING::from(dir.as_os_str());
        // SAFETY: `name` outlives the call; no security attributes and no template handle.
        let dir_handle = unsafe {
            CreateFileW(
                &name,
                FILE_LIST_DIRECTORY.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                None,
            )?
        };
        // SAFETY: an unnamed auto-reset event; no pointers.
        let event = match unsafe { CreateEventW(None, false, false, None) } {
            Ok(event) => event,
            Err(e) => {
                // SAFETY: closes the directory handle opened above, which nothing else holds.
                let _ = unsafe { CloseHandle(dir_handle) };
                return Err(e);
            }
        };
        let io = Box::new(Io {
            ov: OVERLAPPED::default(),
            bytes: 0,
            buf: [0; BUF_WORDS],
        });
        let mut watcher = Self {
            dir: dir_handle,
            event,
            io,
            subtree,
        };
        watcher.arm()?;
        Ok(watcher)
    }

    /// Starts the next read. The event is the read's completion event.
    fn arm(&mut self) -> Result<()> {
        let io = &mut *self.io;
        io.ov.hEvent = self.event;
        // SAFETY: the buffer and the overlapped struct sit in the box until Drop has cancelled the read
        // and waited for it, so the kernel never writes into freed memory. The call is asynchronous, so
        // `bytes` is written only when the read completes, and only while `io` is alive.
        unsafe {
            ReadDirectoryChangesW(
                self.dir,
                io.buf.as_mut_ptr().cast::<c_void>(),
                (BUF_WORDS * std::mem::size_of::<u32>()) as u32,
                self.subtree,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_SIZE,
                Some(&mut io.bytes as *mut u32),
                Some(&mut io.ov as *mut OVERLAPPED),
                None,
            )
        }
    }

    /// Waits up to `timeout` (forever when None) for a change. A change is collected and the watch is
    /// re-armed before this returns, so the next call waits for the next change.
    pub fn wait(&mut self, timeout: Option<Duration>) -> Result<Wake> {
        let ms = timeout.map_or(u32::MAX, |t| {
            t.as_millis().min(u128::from(u32::MAX - 1)) as u32
        });
        // SAFETY: waits on our own event handle; no pointers.
        let signalled = unsafe { WaitForSingleObject(self.event, ms) };
        if signalled == WAIT_TIMEOUT {
            return Ok(Wake::Timeout);
        }
        if signalled != WAIT_OBJECT_0 {
            return Err(Error::from_thread());
        }
        self.collect()?;
        Ok(Wake::Changed)
    }

    /// The event fired: takes the completed read and re-arms.
    fn collect(&mut self) -> Result<()> {
        let io = &mut *self.io;
        // SAFETY: the event fired, so the read has completed and the overlapped struct is final; the
        // byte count goes into `io.bytes`, which lives in the box.
        unsafe { GetOverlappedResult(self.dir, &io.ov, &mut io.bytes, false)? };
        self.arm()
    }
}

/// Waits up to `timeout` (forever when None) for a change in any of `watchers` (at most 64). Returns
/// the index of the watcher that fired and whether it is watching again, or None on timeout. One that
/// is not (its folder went away) just never fires again; the others go on.
pub fn wait_any(
    watchers: &mut [DirWatcher],
    timeout: Option<Duration>,
) -> Result<Option<(usize, bool)>> {
    let ms = timeout.map_or(u32::MAX, |t| {
        t.as_millis().min(u128::from(u32::MAX - 1)) as u32
    });
    let events: Vec<HANDLE> = watchers.iter().map(|w| w.event).collect();
    // SAFETY: waits on the watchers' own event handles, which stay open while the slice is borrowed.
    let r = unsafe { WaitForMultipleObjects(&events, false, ms) };
    if r == WAIT_TIMEOUT {
        return Ok(None);
    }
    let i = r.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
    match watchers.get_mut(i) {
        Some(w) => Ok(Some((i, w.collect().is_ok()))),
        None => Err(Error::from_thread()),
    }
}

impl Drop for DirWatcher {
    fn drop(&mut self) {
        let io = &mut *self.io;
        // SAFETY: cancels this handle's pending read, if there is one; the overlapped struct is alive.
        let _ = unsafe { CancelIoEx(self.dir, Some(&io.ov as *const OVERLAPPED)) };
        // SAFETY: waits until the read has ended, so the kernel no longer writes into `io`.
        let _ = unsafe { GetOverlappedResult(self.dir, &io.ov, &mut io.bytes, true) };
        // SAFETY: closes the two handles this watcher owns, once each.
        unsafe {
            let _ = CloseHandle(self.dir);
            let _ = CloseHandle(self.event);
        }
    }
}
