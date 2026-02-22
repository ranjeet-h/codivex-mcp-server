pub mod config;
pub mod engine;
pub mod queue;
pub mod worker;

pub use config::{EmbeddingConfig, ExecutionDevice, QuantizationMode};
pub use engine::EmbeddingEngine;
pub use queue::{EmbeddingJob, EmbeddingQueue};
pub use worker::{
    EmbeddingWorkerConfig, EmbeddingWorkerMetrics, EmbeddingWorkerMetricsSnapshot,
    run_embedding_worker, run_embedding_worker_with_metrics,
};
