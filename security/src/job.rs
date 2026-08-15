//! Windows Job Object limits for the browser process (practical sandbox).

use tracing::{info, warn};

/// Apply a Job Object to the current process: kill children on close + memory limit.
pub fn apply_job_object() -> Result<(), String> {
    #[cfg(windows)]
    {
        apply_job_object_windows()
    }
    #[cfg(not(windows))]
    {
        info!("Job Object sandbox is Windows-only; skipped");
        Ok(())
    }
}

#[cfg(windows)]
fn apply_job_object_windows() -> Result<(), String> {
    use std::mem::size_of;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::*;
    use windows::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let job = CreateJobObjectW(None, None).map_err(|e| format!("CreateJobObjectW: {e}"))?;
        if job.is_invalid() {
            return Err("CreateJobObjectW returned invalid handle".into());
        }

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        info.ProcessMemoryLimit = 2 * 1024 * 1024 * 1024;

        if let Err(e) = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) {
            warn!("SetInformationJobObject failed: {e}");
            return Err(format!("SetInformationJobObject: {e}"));
        }

        if let Err(e) = AssignProcessToJobObject(job, GetCurrentProcess()) {
            // Already in a job is common under some shells — treat as soft failure
            warn!("AssignProcessToJobObject: {e} (may already be in a job)");
            return Ok(());
        }

        // Intentionally leak handle so JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE stays active.
        let _ = job;
        std::mem::forget(HANDLE(job.0));
        info!("Job Object applied (kill-on-close + 2GiB process memory limit)");
    }
    Ok(())
}
