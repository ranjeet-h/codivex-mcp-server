use std::path::PathBuf;

use anyhow::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher, recommended_watcher};
use tokio::sync::mpsc;

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    pub fn start(paths: &[PathBuf]) -> Result<(Self, mpsc::Receiver<Event>)> {
        let (tx, rx) = mpsc::channel(256);
        let mut watcher = recommended_watcher(move |event| {
            if let Ok(event) = event {
                let _ = tx.blocking_send(event);
            }
        })?;

        for path in paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        Ok((Self { _watcher: watcher }, rx))
    }
}
