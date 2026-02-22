use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct IndexerTelemetry {
    queue_depth: AtomicU64,
    chunks_indexed: AtomicU64,
    last_index_unix_ms: AtomicU64,
    embedded_items: AtomicU64,
}

impl IndexerTelemetry {
    pub fn set_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }

    pub fn inc_chunks_indexed(&self, count: u64) {
        self.chunks_indexed.fetch_add(count, Ordering::Relaxed);
    }

    pub fn set_last_index_unix_ms(&self, timestamp: u64) {
        self.last_index_unix_ms.store(timestamp, Ordering::Relaxed);
    }

    pub fn inc_embedded_items(&self, count: u64) {
        self.embedded_items.fetch_add(count, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> IndexerTelemetrySnapshot {
        IndexerTelemetrySnapshot {
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            chunks_indexed: self.chunks_indexed.load(Ordering::Relaxed),
            last_index_unix_ms: self.last_index_unix_ms.load(Ordering::Relaxed),
            embedded_items: self.embedded_items.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexerTelemetrySnapshot {
    pub queue_depth: u64,
    pub chunks_indexed: u64,
    pub last_index_unix_ms: u64,
    pub embedded_items: u64,
}

#[cfg(test)]
mod tests {
    use super::IndexerTelemetry;

    #[test]
    fn telemetry_snapshot_reflects_updates() {
        let telemetry = IndexerTelemetry::default();
        telemetry.set_queue_depth(9);
        telemetry.inc_chunks_indexed(3);
        telemetry.set_last_index_unix_ms(100);
        telemetry.inc_embedded_items(20);

        let s = telemetry.snapshot();
        assert_eq!(s.queue_depth, 9);
        assert_eq!(s.chunks_indexed, 3);
        assert_eq!(s.last_index_unix_ms, 100);
        assert_eq!(s.embedded_items, 20);
    }
}
