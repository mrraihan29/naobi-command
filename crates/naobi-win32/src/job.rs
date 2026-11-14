//! A job object that kills its processes when its last handle closes. The app puts its transient
//! children (`--icons`, `--search-files`, `--list-apps`) in one, so a crash or a kill of the app, or a
//! child that ignores its stdin closing, never leaves a stray `naobi-app.exe` (and the shell
//! libraries in it) behind.
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub struct Job(HANDLE);

// SAFETY: a job handle is a kernel object handle; it may be used and closed from any thread.
unsafe impl Send for Job {}
// SAFETY: as above; AssignProcessToJobObject is safe to call concurrently.
unsafe impl Sync for Job {}

impl Job {
    /// A new job whose processes die when it is dropped (or when this process ends).
    pub fn kill_on_close() -> windows::core::Result<Self> {
        // SAFETY: an unnamed job with default security; the handle is owned by the returned value.
        let job = Self(unsafe { CreateJobObjectW(None, None)? });
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` is a live, correctly sized extended-limit structure for the class given.
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
        }
        Ok(job)
    }

    /// Puts `child` in the job. False when Windows refused (the child then runs as before).
    pub fn assign(&self, child: &Child) -> bool {
        let process = HANDLE(child.as_raw_handle());
        // SAFETY: both handles are live for the call: the job is owned by `self`, and the process
        // handle is owned by `child`, which the caller holds.
        unsafe { AssignProcessToJobObject(self.0, process) }.is_ok()
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateJobObjectW and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // @R-006
    #[test]
    fn r006_a_child_in_the_job_dies_when_the_job_closes() {
        let mut child = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 120"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn: {e}"));
        let job = Job::kill_on_close().unwrap_or_else(|e| panic!("job: {e}"));
        assert!(job.assign(&child));
        assert!(matches!(child.try_wait(), Ok(None)), "still running");
        drop(job);
        let end = Instant::now() + Duration::from_secs(5);
        let mut gone = false;
        while Instant::now() < end && !gone {
            gone = matches!(child.try_wait(), Ok(Some(_)));
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        assert!(gone, "closing the job ended the child");
    }
}
