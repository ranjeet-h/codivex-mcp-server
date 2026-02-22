use anyhow::{Result, anyhow};
use tree_sitter::{InputEdit, Point, Tree};

use crate::parser_registry::ParserRegistry;

#[derive(Debug, Clone, Copy)]
pub struct ByteEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: Point,
    pub old_end_position: Point,
    pub new_end_position: Point,
}

pub fn incremental_reparse(
    path: &str,
    old_source: &str,
    new_source: &str,
    edit: ByteEdit,
) -> Result<Tree> {
    let registry = ParserRegistry::new();
    let (_, mut parser) = registry.parser_for_path(path)?;
    let mut old_tree = parser
        .parse(old_source, None)
        .ok_or_else(|| anyhow!("failed to parse old source"))?;

    old_tree.edit(&InputEdit {
        start_byte: edit.start_byte,
        old_end_byte: edit.old_end_byte,
        new_end_byte: edit.new_end_byte,
        start_position: edit.start_position,
        old_end_position: edit.old_end_position,
        new_end_position: edit.new_end_position,
    });

    parser
        .parse(new_source, Some(&old_tree))
        .ok_or_else(|| anyhow!("failed incremental parse"))
}

#[cfg(test)]
mod tests {
    use tree_sitter::Point;

    use super::{ByteEdit, incremental_reparse};

    #[test]
    fn incremental_parse_returns_tree() {
        let old_source = "fn a() { 1 }\n";
        let new_source = "fn a() { 2 }\n";
        let edit = ByteEdit {
            start_byte: 9,
            old_end_byte: 10,
            new_end_byte: 10,
            start_position: Point { row: 0, column: 9 },
            old_end_position: Point { row: 0, column: 10 },
            new_end_position: Point { row: 0, column: 10 },
        };
        let tree = incremental_reparse("src/lib.rs", old_source, new_source, edit).expect("tree");
        assert!(tree.root_node().has_changes() || !tree.root_node().is_error());
    }
}
