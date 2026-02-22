use std::{
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use tokio::sync::mpsc;

use crate::{EmbeddingEngine, queue::EmbeddingJob};

#[derive(Debug, Clone)]
pub struct EmbeddingWorkerConfig {
    pub batch_size: usize,
    pub max_retries: usize,
}

impl Default for EmbeddingWorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: 128,
            max_retries: 2,
        }
    }
}

#[derive(Debug, Default)]
pub struct EmbeddingWorkerMetrics {
    batches_processed: AtomicU64,
    items_processed: AtomicU64,
    failures: AtomicU64,
    total_latency_ms: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingWorkerMetricsSnapshot {
    pub batches_processed: u64,
    pub items_processed: u64,
    pub failures: u64,
    pub avg_latency_ms: u64,
}

impl EmbeddingWorkerMetrics {
    fn record_batch(&self, items: usize, latency_ms: u64) {
        self.batches_processed.fetch_add(1, Ordering::Relaxed);
        self.items_processed
            .fetch_add(items as u64, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> EmbeddingWorkerMetricsSnapshot {
        let batches = self.batches_processed.load(Ordering::Relaxed);
        let total = self.total_latency_ms.load(Ordering::Relaxed);
        EmbeddingWorkerMetricsSnapshot {
            batches_processed: batches,
            items_processed: self.items_processed.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            avg_latency_ms: if batches == 0 { 0 } else { total / batches },
        }
    }
}

pub async fn run_embedding_worker(
    rx: mpsc::Receiver<EmbeddingJob>,
    engine: EmbeddingEngine,
    cfg: EmbeddingWorkerConfig,
) {
    run_embedding_worker_with_metrics(rx, engine, cfg, None).await;
}

pub async fn run_embedding_worker_with_metrics(
    mut rx: mpsc::Receiver<EmbeddingJob>,
    engine: EmbeddingEngine,
    cfg: EmbeddingWorkerConfig,
    metrics: Option<Arc<EmbeddingWorkerMetrics>>,
) {
    while let Some(first) = rx.recv().await {
        let mut batch = vec![first];
        while batch.len() < cfg.batch_size {
            match rx.try_recv() {
                Ok(next) => batch.push(next),
                Err(_) => break,
            }
        }

        let texts = batch.iter().map(|j| j.text.clone()).collect::<Vec<_>>();
        let started = Instant::now();
        let mut success = false;
        for _attempt in 0..=cfg.max_retries {
            if engine.embed_batch(&texts).is_ok() {
                success = true;
                break;
            }
        }

        if let Some(metrics) = metrics.as_ref() {
            if success {
                metrics.record_batch(batch.len(), started.elapsed().as_millis() as u64);
            } else {
                metrics.record_failure();
            }
        }

        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        EmbeddingConfig, EmbeddingEngine,
        queue::{EmbeddingJob, EmbeddingQueue},
        worker::{
            EmbeddingWorkerConfig, EmbeddingWorkerMetrics, run_embedding_worker_with_metrics,
        },
    };

    #[tokio::test]
    async fn worker_processes_queue_until_channel_closes() {
        let (queue, rx) = EmbeddingQueue::new(8);
        queue
            .enqueue(EmbeddingJob {
                chunk_id: "c1".to_string(),
                text: "hello".to_string(),
            })
            .await
            .expect("enqueue");
        drop(queue);

        let engine = EmbeddingEngine::new(EmbeddingConfig::default());
        let metrics = std::sync::Arc::new(EmbeddingWorkerMetrics::default());
        run_embedding_worker_with_metrics(
            rx,
            engine,
            EmbeddingWorkerConfig::default(),
            Some(metrics.clone()),
        )
        .await;
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.items_processed, 1);
    }
}
