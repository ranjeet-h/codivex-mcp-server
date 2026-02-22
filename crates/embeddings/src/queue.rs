use anyhow::Result;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct EmbeddingJob {
    pub chunk_id: String,
    pub text: String,
}

pub struct EmbeddingQueue {
    tx: mpsc::Sender<EmbeddingJob>,
}

impl EmbeddingQueue {
    pub fn new(buffer: usize) -> (Self, mpsc::Receiver<EmbeddingJob>) {
        let (tx, rx) = mpsc::channel(buffer);
        (Self { tx }, rx)
    }

    pub async fn enqueue(&self, job: EmbeddingJob) -> Result<()> {
        self.tx.send(job).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EmbeddingJob, EmbeddingQueue};

    #[tokio::test]
    async fn enqueue_and_receive_job() {
        let (queue, mut rx) = EmbeddingQueue::new(4);
        queue
            .enqueue(EmbeddingJob {
                chunk_id: "1".to_string(),
                text: "hello".to_string(),
            })
            .await
            .expect("enqueue");

        let got = rx.recv().await.expect("received");
        assert_eq!(got.chunk_id, "1");
        assert_eq!(got.text, "hello");
    }
}
