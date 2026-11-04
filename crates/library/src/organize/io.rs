use super::{OrganizeOutcome, OrganizePlan};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub fn apply_plan(plan: &OrganizePlan) -> OrganizeOutcome {
    let mut outcome = OrganizeOutcome::default();

    for entry in plan.actionable() {
        if let Some(parent) = entry.to.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                outcome.failures.push((
                    entry.from.clone(),
                    format!("cannot create {}: {e}", parent.display()),
                ));
                continue;
            }
        }

        match move_file(&entry.from, &entry.to) {
            Ok(()) => {
                outcome
                    .moved
                    .push((entry.track_id, entry.to.clone(), entry.content_hash.clone()))
            }
            Err(e) => outcome
                .failures
                .push((entry.from.clone(), describe_move_error(&e))),
        }
    }

    outcome
}

/// Move a file, never replacing an existing destination and working across
/// filesystems.
///
/// `fs::rename` does neither. On Unix it silently replaces an existing
/// destination — the plan checks for conflicts, but the plan was made minutes
/// ago and the disk is free to change underneath it — and across filesystems it
/// fails outright, so organizing a library spread over two drives simply did
/// not work.
///
/// A hard link is the same-filesystem case: it cannot clobber (`AlreadyExists`
/// is an error, not a replacement) and it never copies the bytes. When linking
/// is impossible — another filesystem, or a filesystem that does not support
/// links — the file is copied with `create_new` and the copy is durable before
/// the original is unlinked.
pub(crate) fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => {
            // The destination now names the same file. Dropping the old name
            // cannot lose data: if this fails the two names still agree.
            fs::remove_file(from)
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(e),
        Err(_) => copy_then_delete(from, to),
    }
}

/// The cross-filesystem fallback: copy, flush, then unlink the source.
pub(crate) fn copy_then_delete(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::io::Write;

    let mut source = fs::File::open(from)?;
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;

    if let Err(e) = std::io::copy(&mut source, &mut destination)
        .and_then(|_| destination.flush())
        .and_then(|_| destination.sync_all())
    {
        // Never leave a partial copy at the destination: the next run would
        // see it as a conflict and the library would gain a truncated file.
        drop(destination);
        let _ = fs::remove_file(to);
        return Err(e);
    }
    drop(destination);

    fs::remove_file(from)
}

pub(crate) fn describe_move_error(e: &std::io::Error) -> String {
    if e.kind() == ErrorKind::CrossesDevices {
        return "destination is on another filesystem; pick a folder on the same drive".to_string();
    }
    if e.kind() == ErrorKind::AlreadyExists {
        return "a file already exists there (not overwritten)".to_string();
    }
    if e.kind() == ErrorKind::PermissionDenied {
        return "permission denied".to_string();
    }
    e.to_string()
}
