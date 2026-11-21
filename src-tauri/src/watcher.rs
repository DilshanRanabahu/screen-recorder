//! Keeps the studio's "Recent recordings" list in sync with the recordings
//! folder, including changes made outside the app (Explorer, other tools).
//!
//! A background thread watches the folder with `ReadDirectoryChangesW` (via
//! the `notify` crate) and sends the UI a `recordings-changed` event; the UI
//! then re-reads the list. Events are batched: a refresh is sent once the
//! folder has been quiet for 400 ms, or at least every 2 s while changes keep
//! coming (e.g. a recording that is still growing).
//!
//! If the folder is deleted or the watch fails, the thread recreates the
//! folder and attaches again, so the list never stays stale until restart.

use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "recordings-changed";

/// Send a refresh once the folder has been quiet this long…
const QUIET: Duration = Duration::from_millis(400);
/// …but never wait longer than this while events keep arriving.
const MAX_DELAY: Duration = Duration::from_secs(2);
/// How often to check that the folder still exists when nothing happens.
const HEALTH_CHECK: Duration = Duration::from_secs(5);
/// Pause before re-attaching after the watch was lost.
const RETRY: Duration = Duration::from_secs(2);

/// Starts the watcher thread for the recordings folder.
pub fn spawn(app: AppHandle) {
    let result = std::thread::Builder::new().name("recordings-watcher".into()).spawn(move || {
        loop {
            let dir = recorder_core::default_output_dir();
            match watch_until_lost(&app, &dir) {
                Ok(()) => tracing::info!(dir = %dir.display(), "recordings folder went away; re-attaching"),
                Err(e) => tracing::warn!(dir = %dir.display(), error = %e, "recordings folder watch failed; retrying"),
            }
            // Whatever happened, the list may be out of date now.
            notify_changed(&app);
            std::thread::sleep(RETRY);
        }
    });
    if let Err(e) = result {
        tracing::error!(error = %e, "could not start the recordings folder watcher");
    }
}

fn notify_changed(app: &AppHandle) {
    tracing::debug!("recordings folder changed");
    if let Err(e) = app.emit(EVENT, ()) {
        tracing::debug!(error = %e, "failed to emit {EVENT}");
    }
}

/// Watches `dir` until it disappears or the watch fails.
fn watch_until_lost(app: &AppHandle, dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    tracing::info!(dir = %dir.display(), "watching recordings folder");
    // Anything may have changed while we weren't watching.
    notify_changed(app);

    // (first event, last event) of the batch not yet reported.
    let mut pending: Option<(Instant, Instant)> = None;
    loop {
        let timeout = match pending {
            Some((first, last)) => QUIET.saturating_sub(last.elapsed()).min(MAX_DELAY.saturating_sub(first.elapsed())),
            None => HEALTH_CHECK,
        };
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                if is_relevant(&event, dir) {
                    let now = Instant::now();
                    pending = Some(pending.map_or((now, now), |(first, _)| (first, now)));
                }
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(RecvTimeoutError::Timeout) => {
                if pending.take().is_some() {
                    notify_changed(app);
                }
                if !dir.is_dir() {
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        // Flush a long-running batch even if events never pause.
        if let Some((first, last)) = pending
            && (first.elapsed() >= MAX_DELAY || last.elapsed() >= QUIET)
        {
            pending = None;
            notify_changed(app);
        }
    }
}

/// Changes that can affect the list: MP4 files appearing, disappearing,
/// being renamed or changing size, or the folder itself changing. Reads
/// (`Access`) are ignored. Events without paths mean the OS dropped details
/// (buffer overflow), so they always trigger a refresh.
fn is_relevant(event: &Event, dir: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.is_empty()
        || event.paths.iter().any(|p| {
            p == dir || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};
    use std::path::PathBuf;

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        let mut e = Event::new(kind);
        e.paths = paths.iter().map(PathBuf::from).collect();
        e
    }

    #[test]
    fn video_changes_are_relevant() {
        let dir = Path::new(r"C:\rec");
        assert!(is_relevant(&event(EventKind::Create(CreateKind::File), &[r"C:\rec\a.mp4"]), dir));
        assert!(is_relevant(&event(EventKind::Remove(RemoveKind::File), &[r"C:\rec\a.MP4"]), dir));
        assert!(is_relevant(&event(EventKind::Modify(ModifyKind::Name(RenameMode::Both)), &[r"C:\rec\a.mp4", r"C:\rec\b.mp4"]), dir));
        assert!(is_relevant(&event(EventKind::Remove(RemoveKind::Folder), &[r"C:\rec"]), dir));
        // No paths: the OS dropped details, so resync.
        assert!(is_relevant(&event(EventKind::Any, &[]), dir));
    }

    #[test]
    fn other_changes_are_ignored() {
        let dir = Path::new(r"C:\rec");
        assert!(!is_relevant(&event(EventKind::Create(CreateKind::File), &[r"C:\rec\notes.txt"]), dir));
        assert!(!is_relevant(&event(EventKind::Access(AccessKind::Any), &[r"C:\rec\a.mp4"]), dir));
    }
}
