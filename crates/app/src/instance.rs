//! Single-instance guard and second-launch activation.
//!
//! The lock is an OS advisory lock on `<data_dir>/instance.lock`, held for the
//! life of the process. The kernel releases it on exit (including a crash), so
//! there is no stale-PID file to clean up. A second launch that cannot take the
//! lock becomes a controller: it reads the owner's loopback endpoint from
//! `<data_dir>/instance.json` and sends `raise`, then exits.
//!
//! The multi-instance opt-in is read from a marker file before the database is
//! opened, because the decision has to be made before the second process
//! touches the SQLite file or the audio device.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// The lock held by the single owning instance.
pub struct InstanceLock {
    file: File,
    path: PathBuf,
}

/// Result of trying to become the owner.
pub enum LockOutcome {
    /// This process is the owner and must keep this guard alive.
    Acquired(InstanceLock),
    /// Another process holds the lock; it should be activated instead.
    AlreadyRunning,
}

impl InstanceLock {
    /// Try to take the instance lock without blocking.
    pub fn acquire(data_dir: &Path) -> io::Result<LockOutcome> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("instance.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        secure_permissions(&path);

        match file.try_lock() {
            Ok(()) => Ok(LockOutcome::Acquired(Self { file, path })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(LockOutcome::AlreadyRunning),
            Err(std::fs::TryLockError::Error(e)) => Err(io::Error::new(
                e.kind(),
                format!("could not lock the instance file {}: {e}", path.display()),
            )),
        }
    }

    /// Path of the lock file, for diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // Explicit unlock is best-effort; closing the file releases it too.
        let _ = self.file.unlock();
    }
}

fn secure_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_instance_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_second_acquire_reports_the_owner() {
        let dir = temp_dir("lock");
        let first = InstanceLock::acquire(&dir).unwrap();
        assert!(matches!(first, LockOutcome::Acquired(_)));
        let second = InstanceLock::acquire(&dir).unwrap();
        assert!(
            matches!(second, LockOutcome::AlreadyRunning),
            "a second process must become a controller, not a second owner"
        );
        drop(first);
        let third = InstanceLock::acquire(&dir).unwrap();
        assert!(
            matches!(third, LockOutcome::Acquired(_)),
            "the kernel releases the lock when the owner exits"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn activation_reaches_the_control_endpoint() {
        let dir = temp_dir("activate");
        let service = ui::services::ControlService::start(&dir).unwrap();
        ui::services::activate_existing_instance(&dir).unwrap();
        assert_eq!(
            service.try_recv(),
            Some(ui::services::MprisCommand::Raise),
            "a second launch asks the owner to come forward"
        );
        drop(service);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
