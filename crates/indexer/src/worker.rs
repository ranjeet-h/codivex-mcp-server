use std::path::PathBuf;

use anyhow::Result;
use rayon::prelude::*;
use tokio::sync::mpsc;

pub struct IndexWorkQueue {
    tx: mpsc::Sender<PathBuf>,
}

impl IndexWorkQueue {
    pub fn new(buffer: usize) -> (Self, mpsc::Receiver<PathBuf>) {
        let (tx, rx) = mpsc::channel(buffer);
        (Self { tx }, rx)
    }

    pub async fn enqueue(&self, path: PathBuf) -> Result<()> {
        self.tx.send(path).await?;
        Ok(())
    }
}

pub async fn run_worker(mut rx: mpsc::Receiver<PathBuf>) {
    while let Some(path) = rx.recv().await {
        // CPU-heavy indexing work is delegated to rayon threads.
        let _ = rayon::spawn(move || {
            let _normalized = normalize_paths_for_batch(vec![path]);
        });
    }
}

fn normalize_paths_for_batch(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .par_iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::normalize_paths_for_batch;

    #[test]
    fn normalizes_paths_with_parallel_worker() {
        let out = normalize_paths_for_batch(vec![PathBuf::from("src\\main.rs")]);
        assert_eq!(out, vec!["src/main.rs".to_string()]);
    }
}
