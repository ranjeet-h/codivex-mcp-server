use common::CodeChunk;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOperation {
    Upsert { chunk_id: String },
    Delete { chunk_id: String },
}

pub fn plan_sync_operations(
    new_chunks: &[CodeChunk],
    modified_chunks: &[CodeChunk],
    deleted_chunk_ids: &[String],
) -> Vec<SyncOperation> {
    let mut ops = Vec::new();
    ops.extend(new_chunks.iter().map(|c| SyncOperation::Upsert {
        chunk_id: c.id.clone(),
    }));
    ops.extend(modified_chunks.iter().map(|c| SyncOperation::Upsert {
        chunk_id: c.id.clone(),
    }));
    ops.extend(
        deleted_chunk_ids
            .iter()
            .cloned()
            .map(|id| SyncOperation::Delete { chunk_id: id }),
    );
    ops
}

#[cfg(test)]
mod tests {
    use common::CodeChunk;

    use super::{SyncOperation, plan_sync_operations};

    fn chunk(id: &str) -> CodeChunk {
        CodeChunk {
            id: id.to_string(),
            fingerprint: "fp".to_string(),
            file_path: "src/a.rs".to_string(),
            language: "rust".to_string(),
            symbol: None,
            start_line: 1,
            end_line: 1,
            start_char: 0,
            end_char: 0,
            content: "fn a() {}".to_string(),
        }
    }

    #[test]
    fn builds_upsert_and_delete_plan() {
        let ops = plan_sync_operations(&[chunk("n1")], &[chunk("m1")], &["d1".to_string()]);
        assert_eq!(
            ops,
            vec![
                SyncOperation::Upsert {
                    chunk_id: "n1".to_string()
                },
                SyncOperation::Upsert {
                    chunk_id: "m1".to_string()
                },
                SyncOperation::Delete {
                    chunk_id: "d1".to_string()
                }
            ]
        );
    }
}
