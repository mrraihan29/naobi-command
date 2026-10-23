use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// One reading of a process: memory in bytes, CPU time in 100 ns units (kernel + user).
#[derive(Debug, Clone, Copy)]
pub struct ProcessMetrics {
    pub working_set: u64,
    pub private_bytes: u64,
    pub cpu_100ns: u64,
}

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenProcess and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn ticks(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}

impl ProcessMetrics {
    pub fn sample(pid: u32) -> windows::core::Result<Self> {
        // SAFETY: plain value arguments; the returned handle is owned and closed by `Owned`.
        let h = Owned(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? });
        let mut mem = PROCESS_MEMORY_COUNTERS_EX::default();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        // SAFETY: `mem` is a live, correctly sized PROCESS_MEMORY_COUNTERS_EX; the API fills
        // it through the base-struct pointer and writes at most `size` bytes.
        unsafe { GetProcessMemoryInfo(h.0, std::ptr::from_mut(&mut mem).cast(), size)? };
        let (mut create, mut exit, mut kernel, mut user) = Default::default();
        // SAFETY: all four out-pointers refer to live FILETIME locals.
        unsafe { GetProcessTimes(h.0, &mut create, &mut exit, &mut kernel, &mut user)? };
        Ok(Self {
            working_set: mem.WorkingSetSize as u64,
            private_bytes: mem.PrivateUsage as u64,
            cpu_100ns: ticks(kernel) + ticks(user),
        })
    }

    pub fn sample_self() -> windows::core::Result<Self> {
        // SAFETY: GetCurrentProcessId takes no arguments and cannot fail.
        Self::sample(unsafe { GetCurrentProcessId() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_self_reports_memory() {
        let m = ProcessMetrics::sample_self().unwrap();
        assert!(m.working_set > 0 && m.private_bytes > 0);
        assert!(m.working_set < 1 << 32);
    }
}
