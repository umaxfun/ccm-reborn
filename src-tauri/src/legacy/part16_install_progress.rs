use std::{sync::Mutex, time::{Duration, Instant}};
use tauri::Emitter;

const INSTALL_PROGRESS_EVENT: &str = "install-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    operation_id: String,
    phase: String,
    message: String,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
    completed_files: Option<usize>,
    total_files: Option<usize>,
}

/// Reports useful, throttled installation state to the desktop window. The
/// same reporter writes concise lifecycle entries to the diagnostic log, so a
/// user can report a failed download without reproducing it in a terminal.
struct ProgressReporter {
    operation_id: String,
    window: tauri::Window,
    last_download_event: Mutex<Option<(u64, Instant)>>,
}

impl ProgressReporter {
    fn new(window: tauri::Window) -> Self {
        Self {
            operation_id: Uuid::new_v4().to_string(),
            window,
            last_download_event: Mutex::new(None),
        }
    }

    fn status(&self, phase: &str, message: impl Into<String>) {
        let message = message.into();
        self.emit(phase, &message, None, None, None, None);
        append_diagnostic_log(&format!("operation={} phase={} {}", self.operation_id, phase, message));
    }

    fn download(&self, downloaded_bytes: u64, total_bytes: u64) {
        let now = Instant::now();
        let should_emit = self
            .last_download_event
            .lock()
            .map(|mut last| {
                let changed_enough = last.as_ref().map_or(true, |(bytes, _)| {
                    downloaded_bytes.saturating_sub(*bytes) >= 256 * 1024
                });
                let waited_long_enough = last.as_ref().map_or(true, |(_, instant)| {
                    now.duration_since(*instant) >= Duration::from_millis(250)
                });
                let complete = downloaded_bytes >= total_bytes;
                if (changed_enough && waited_long_enough) || complete {
                    *last = Some((downloaded_bytes, now));
                    true
                } else {
                    false
                }
            })
            .unwrap_or(true);
        if should_emit {
            self.emit(
                "downloading",
                "Downloading package from cloud…",
                Some(downloaded_bytes),
                Some(total_bytes),
                None,
                None,
            );
        }
    }

    fn files(&self, phase: &str, message: impl Into<String>, completed_files: usize, total_files: usize) {
        self.emit(phase, &message.into(), None, None, Some(completed_files), Some(total_files));
    }

    fn failed(&self, phase: &str, error: &str) {
        self.status(phase, "Operation stopped. Diagnostic details were written to the CCM log.");
        append_diagnostic_log(&format!("operation={} phase={} error={}", self.operation_id, phase, redact_diagnostic_value(error)));
    }

    fn operation_id(&self) -> &str {
        &self.operation_id
    }

    fn emit(
        &self,
        phase: &str,
        message: &str,
        downloaded_bytes: Option<u64>,
        total_bytes: Option<u64>,
        completed_files: Option<usize>,
        total_files: Option<usize>,
    ) {
        let _ = self.window.emit(
            INSTALL_PROGRESS_EVENT,
            InstallProgress {
                operation_id: self.operation_id.clone(),
                phase: phase.into(),
                message: message.into(),
                downloaded_bytes,
                total_bytes,
                completed_files,
                total_files,
            },
        );
    }
}
