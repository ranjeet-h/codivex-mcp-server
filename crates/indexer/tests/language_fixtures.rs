use indexer::extract_chunks_for_file;

#[test]
fn fixture_corpus_extracts_chunks_for_supported_languages() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/languages");
    let cases = vec![
        ("sample.rs", "rust"),
        ("sample.c", "c"),
        ("sample.cpp", "cpp"),
        ("sample.js", "javascript"),
        ("sample.ts", "typescript"),
        ("sample.py", "python"),
        ("sample.go", "go"),
        ("sample.hs", "haskell"),
        ("Sample.java", "java"),
        ("Sample.cs", "csharp"),
        ("sample.php", "php"),
        ("sample.rb", "ruby"),
        ("sample.kt", "kotlin"),
        ("sample.swift", "swift"),
    ];

    for (file, expected_language) in cases {
        let path = base.join(file);
        let content = std::fs::read_to_string(&path).expect("fixture content");
        let chunks = extract_chunks_for_file(path.to_string_lossy().as_ref(), &content)
            .unwrap_or_else(|err| panic!("failed to parse {file}: {err}"));
        assert!(!chunks.is_empty(), "no chunks extracted for {file}");
        assert_eq!(
            chunks[0].language, expected_language,
            "unexpected language for {file}"
        );
    }
}
