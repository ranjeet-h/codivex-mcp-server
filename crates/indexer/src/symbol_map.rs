use ahash::AHashMap;
use common::CodeChunk;

#[derive(Default)]
pub struct SymbolMap {
    by_symbol: AHashMap<String, CodeChunk>,
}

impl SymbolMap {
    pub fn insert(&mut self, chunk: CodeChunk) {
        if let Some(symbol) = &chunk.symbol {
            self.by_symbol.insert(symbol.clone(), chunk);
        }
    }

    pub fn get(&self, symbol: &str) -> Option<&CodeChunk> {
        self.by_symbol.get(symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::SymbolMap;
    use common::CodeChunk;

    #[test]
    fn exact_lookup_is_available() {
        let mut map = SymbolMap::default();
        let chunk = CodeChunk {
            id: "1".to_string(),
            fingerprint: "fp".to_string(),
            file_path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            symbol: Some("foo".to_string()),
            start_line: 1,
            end_line: 2,
            start_char: 0,
            end_char: 10,
            content: "fn foo() {}".to_string(),
        };
        map.insert(chunk);
        assert!(map.get("foo").is_some());
    }
}
