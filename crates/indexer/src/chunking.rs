use anyhow::{Result, anyhow};
use common::CodeChunk;
use tree_sitter::{Node, TreeCursor};

use crate::{
    fingerprint::fingerprint_content,
    parser_registry::{LanguageKind, ParserRegistry},
};

pub fn extract_chunks_for_file(path: &str, content: &str) -> Result<Vec<CodeChunk>> {
    let registry = ParserRegistry::new();
    let (kind, mut parser) = registry.parser_for_path(path)?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("failed to parse source"))?;
    let root = tree.root_node();

    let mut chunks = Vec::new();
    let mut cursor = root.walk();
    collect_chunks(
        path,
        kind,
        content,
        &mut cursor,
        &mut chunks,
        &tree.root_node(),
    );

    if chunks.is_empty() {
        chunks.push(file_chunk(path, kind, content, root));
    }

    Ok(chunks)
}

fn collect_chunks(
    path: &str,
    kind: LanguageKind,
    content: &str,
    cursor: &mut TreeCursor<'_>,
    out: &mut Vec<CodeChunk>,
    root: &Node<'_>,
) {
    loop {
        let node = cursor.node();
        if is_chunk_candidate(kind, node.kind()) {
            out.push(node_chunk(path, kind, content, node, root));
        }

        if cursor.goto_first_child() {
            collect_chunks(path, kind, content, cursor, out, root);
            let _ = cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn is_chunk_candidate(kind: LanguageKind, node_kind: &str) -> bool {
    match kind {
        LanguageKind::Rust => matches!(node_kind, "function_item" | "impl_item" | "struct_item"),
        LanguageKind::C | LanguageKind::Cpp => matches!(
            node_kind,
            "function_definition" | "declaration" | "struct_specifier" | "class_specifier"
        ),
        LanguageKind::JavaScript | LanguageKind::TypeScript => {
            matches!(
                node_kind,
                "function_declaration" | "method_definition" | "class_declaration"
            )
        }
        LanguageKind::Python => matches!(node_kind, "function_definition" | "class_definition"),
        LanguageKind::Go => matches!(node_kind, "function_declaration" | "method_declaration"),
        LanguageKind::Haskell => matches!(
            node_kind,
            "function"
                | "signature"
                | "data_type"
                | "newtype"
                | "type_family"
                | "class"
                | "instance"
        ),
        LanguageKind::Java => matches!(
            node_kind,
            "method_declaration" | "class_declaration" | "interface_declaration"
        ),
        LanguageKind::CSharp => matches!(
            node_kind,
            "method_declaration"
                | "constructor_declaration"
                | "class_declaration"
                | "interface_declaration"
        ),
        LanguageKind::Php => matches!(
            node_kind,
            "function_definition" | "method_declaration" | "class_declaration"
        ),
        LanguageKind::Ruby => matches!(node_kind, "method" | "singleton_method" | "class"),
        LanguageKind::Kotlin => matches!(
            node_kind,
            "function_declaration" | "class_declaration" | "object_declaration"
        ),
        LanguageKind::Swift => matches!(
            node_kind,
            "function_declaration" | "class_declaration" | "struct_declaration"
        ),
    }
}

fn node_chunk(
    path: &str,
    kind: LanguageKind,
    content: &str,
    node: Node<'_>,
    _root: &Node<'_>,
) -> CodeChunk {
    let start = with_leading_comment_start(content, node.start_byte());
    let end = node.end_byte();
    let snippet = content.get(start..end).unwrap_or_default().to_string();

    let symbol = node
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(content.as_bytes()).ok())
        .map(ToOwned::to_owned);

    CodeChunk {
        id: format!(
            "{}:{}:{}",
            path,
            node.start_position().row,
            node.end_position().row
        ),
        fingerprint: fingerprint_content(&snippet),
        file_path: path.to_string(),
        language: kind.label().to_string(),
        symbol,
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_char: start,
        end_char: end,
        content: snippet,
    }
}

fn file_chunk(path: &str, kind: LanguageKind, content: &str, root: Node<'_>) -> CodeChunk {
    CodeChunk {
        id: format!(
            "{}:{}:{}",
            path,
            root.start_position().row,
            root.end_position().row
        ),
        fingerprint: fingerprint_content(content),
        file_path: path.to_string(),
        language: kind.label().to_string(),
        symbol: None,
        start_line: root.start_position().row + 1,
        end_line: root.end_position().row + 1,
        start_char: root.start_byte(),
        end_char: root.end_byte(),
        content: content.to_string(),
    }
}

fn with_leading_comment_start(content: &str, node_start: usize) -> usize {
    let prefix = &content[..node_start.min(content.len())];
    let lines = prefix.lines().collect::<Vec<_>>();
    let mut line_index = lines.len();
    while line_index > 0 {
        let line = lines[line_index - 1].trim();
        if line.starts_with("//") || line.starts_with("///") || line.is_empty() {
            line_index -= 1;
            continue;
        }
        break;
    }
    lines[..line_index]
        .iter()
        .map(|l| l.len() + 1)
        .sum::<usize>()
        .min(node_start)
}

#[cfg(test)]
mod tests {
    use super::extract_chunks_for_file;

    #[test]
    fn extracts_function_chunks_from_rust_file() {
        let content = r#"
        /// docs
        fn iso_to_date() { println!("ok"); }
        fn parse_date() { println!("ok"); }
        "#;
        let chunks = extract_chunks_for_file("src/date.rs", content).expect("chunks");
        assert!(chunks.len() >= 2);
        assert!(
            chunks
                .iter()
                .any(|c| c.symbol.as_deref() == Some("iso_to_date"))
        );
    }

    #[test]
    fn extracts_chunks_for_javascript() {
        let content = "function saveUser() { return true; }";
        let chunks = extract_chunks_for_file("src/app.js", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "javascript");
    }

    #[test]
    fn extracts_chunks_for_typescript() {
        let content = "class Repo { save(): void {} }";
        let chunks = extract_chunks_for_file("src/app.ts", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "typescript");
    }

    #[test]
    fn extracts_chunks_for_python() {
        let content = "def save_user():\n    return True\n";
        let chunks = extract_chunks_for_file("src/app.py", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "python");
    }

    #[test]
    fn extracts_chunks_for_go() {
        let content = "package main\nfunc SaveUser() bool { return true }\n";
        let chunks = extract_chunks_for_file("src/app.go", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "go");
    }

    #[test]
    fn extracts_chunks_for_c() {
        let content = "int save_user(int id) { return id; }\n";
        let chunks = extract_chunks_for_file("src/app.c", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "c");
    }

    #[test]
    fn extracts_chunks_for_java() {
        let content = "class Repo { int saveUser() { return 1; } }";
        let chunks = extract_chunks_for_file("src/Repo.java", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "java");
    }

    #[test]
    fn extracts_chunks_for_csharp() {
        let content = "class Repo { int SaveUser() { return 1; } }";
        let chunks = extract_chunks_for_file("src/Repo.cs", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "csharp");
    }

    #[test]
    fn extracts_chunks_for_kotlin() {
        let content = "class Repo { fun saveUser(): Int = 1 }";
        let chunks = extract_chunks_for_file("src/Repo.kt", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "kotlin");
    }

    #[test]
    fn extracts_chunks_for_haskell() {
        let content = r#"
        module Date where
        isoToDate :: String -> String
        isoToDate input = input
        "#;
        let chunks = extract_chunks_for_file("src/Date.hs", content).expect("chunks");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].language, "haskell");
    }
}
