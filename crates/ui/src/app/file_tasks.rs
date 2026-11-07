use super::*;

impl ShadowPlayerApp {
    /// Apply finished disk work from the file worker.
    ///
    /// The worker decides nothing: it reports what the disk did, and the
    /// messages raised here describe exactly that. A tag write that failed is
    /// a warning, not a success with a footnote.
    pub(super) fn poll_file_tasks(&mut self) {
        for event in self.file_tasks.poll() {
            match event {
                FileTaskEvent::TagsWritten {
                    path,
                    success_message,
                    error,
                } => match error {
                    Some(error) => self.toasts.warning(tr_args!(
                        "Could not write the tags in {file}: {error}",
                        "file" => path.display().to_string(),
                        "error" => error
                    )),
                    None => {
                        if let Some(message) = success_message {
                            self.toasts.success(message);
                        }
                    }
                },
                FileTaskEvent::TagsBatchWritten {
                    written,
                    failures,
                    success_message,
                } => {
                    for failure in failures.iter().take(3) {
                        log::warn!("Tag write-back failed: {failure}");
                    }
                    if failures.is_empty() {
                        self.toasts.success(success_message);
                    } else {
                        self.toasts.warning(tr_args!(
                            "Tags applied to {written} track(s); {failed} could not be written",
                            "written" => written,
                            "failed" => failures.len()
                        ));
                    }
                }
                FileTaskEvent::TracksDeleted {
                    removed,
                    delete_from_disk,
                    failures,
                } => {
                    for failure in failures.iter().take(3) {
                        log::warn!("Duplicate deletion failed: {failure}");
                    }
                    log::info!("Removed {removed} duplicate track(s)");
                    if removed > 0 {
                        self.toasts.success(if delete_from_disk {
                            tr_args!("Deleted {count} duplicate track(s)", "count" => removed)
                        } else {
                            tr_args!("Removed {count} duplicate track(s)", "count" => removed)
                        });
                    }
                    if !failures.is_empty() {
                        self.toasts.warning(tr_args!(
                            "{failed} file(s) could not be deleted",
                            "failed" => failures.len()
                        ));
                    }
                    self.reload_library();
                    // Refresh so the list reflects what is left.
                    self.scan_duplicates();
                }
                FileTaskEvent::OrganizeApplied {
                    outcome,
                    relinked,
                    relink_failures,
                } => {
                    self.finish_organize(outcome, relinked, relink_failures);
                }
                FileTaskEvent::FileCopied {
                    destination,
                    success_message,
                    error,
                } => match error {
                    Some(error) => self.toasts.warning(tr_args!(
                        "Could not save the measurement: {error}",
                        "error" => error
                    )),
                    None => {
                        log::info!("Copied to {}", destination.display());
                        self.toasts.success(success_message);
                    }
                },
                FileTaskEvent::TextWritten {
                    path,
                    success_message,
                    error,
                } => match error {
                    Some(error) => self.toasts.warning(tr_args!(
                        "Could not write {path}: {error}",
                        "path" => path.display().to_string(),
                        "error" => error
                    )),
                    None => self.toasts.success(success_message),
                },
                FileTaskEvent::HistoryExported(result) => match result {
                    Ok(Some(path)) => self.toasts.success(tr_args!(
                        "Exported the listening history to {path}",
                        "path" => path.display().to_string()
                    )),
                    Ok(None) => {}
                    Err(error) => self.toasts.warning(error),
                },
            }
        }
    }
}
