use std::{fs, path::PathBuf};

use common::CodeChunk;
use search_core::lexical::TantivyLexicalIndex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RelevanceCase {
    query: String,
    chunk_id: String,
    file_path: String,
    symbol: String,
    content: String,
}

#[test]
fn lexical_relevance_harness_matches_fixture_expectations() {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("relevance.json");
    let raw = fs::read_to_string(fixture_path).expect("fixture");
    let cases: Vec<RelevanceCase> = serde_json::from_str(&raw).expect("parse fixture");

    let mut index = TantivyLexicalIndex::new_in_memory().expect("index");
    for case in &cases {
        index
            .add_chunk(&CodeChunk {
                id: case.chunk_id.clone(),
                fingerprint: "fp".to_string(),
                file_path: case.file_path.clone(),
                language: "rust".to_string(),
                symbol: Some(case.symbol.clone()),
                start_line: 1,
                end_line: 3,
                start_char: 0,
                end_char: case.content.len(),
                content: case.content.clone(),
            })
            .expect("add");
    }
    index.commit().expect("commit");

    for case in &cases {
        let ids = index.search_ids(&case.query, 1).expect("search");
        assert_eq!(ids.first(), Some(&case.chunk_id));
    }
}
