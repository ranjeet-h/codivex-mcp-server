use ahash::AHashMap;
use common::CodeChunk;

#[derive(Default)]
pub struct FingerprintStore {
    by_chunk_id: AHashMap<String, String>,
}

impl FingerprintStore {
    pub fn should_index(&mut self, chunk: &CodeChunk) -> bool {
        match self.by_chunk_id.get(&chunk.id) {
            Some(existing) if existing == &chunk.fingerprint => false,
            _ => {
                self.by_chunk_id
                    .insert(chunk.id.clone(), chunk.fingerprint.clone());
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use common::CodeChunk;

    use super::FingerprintStore;

    fn chunk(id: &str, fingerprint: &str) -> CodeChunk {
        CodeChunk {
            id: id.to_string(),
            fingerprint: fingerprint.to_string(),
            file_path: "src/a.rs".to_string(),
            language: "rust".to_string(),
            symbol: None,
            start_line: 1,
            end_line: 1,
            start_char: 0,
            end_char: 10,
            content: "fn a() {}".to_string(),
        }
    }

    #[test]
    fn unchanged_chunks_are_skipped() {
        let mut store = FingerprintStore::default();
        assert!(store.should_index(&chunk("1", "abc")));
        assert!(!store.should_index(&chunk("1", "abc")));
        assert!(store.should_index(&chunk("1", "def")));
    }
}
