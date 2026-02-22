use anyhow::Result;
use common::CodeChunk;
use std::path::Path;
use tantivy::schema::Value;
use tantivy::{
    Index, IndexReader, IndexWriter, TantivyDocument,
    collector::TopDocs,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, TEXT},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalSearchConfig {
    pub default_top_k: usize,
}

impl Default for LexicalSearchConfig {
    fn default() -> Self {
        Self { default_top_k: 20 }
    }
}

pub struct TantivyLexicalIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    fields: TantivyFields,
}

#[derive(Clone, Copy)]
struct TantivyFields {
    id: Field,
    path: Field,
    symbol: Field,
    content: Field,
}

impl TantivyLexicalIndex {
    pub fn new_in_memory() -> Result<Self> {
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());
        from_index(index)
    }

    pub fn open_or_create_on_disk(index_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(index_dir)?;
        let schema = build_schema();
        let meta = index_dir.join("meta.json");
        let index = if meta.exists() {
            Index::open_in_dir(index_dir)?
        } else {
            Index::create_in_dir(index_dir, schema)?
        };
        from_index(index)
    }

    pub fn reset(&mut self) -> Result<()> {
        self.writer.delete_all_documents()?;
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn add_chunk(&mut self, chunk: &CodeChunk) -> Result<()> {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.fields.id, &chunk.id);
        doc.add_text(self.fields.path, &chunk.file_path);
        doc.add_text(self.fields.symbol, chunk.symbol.as_deref().unwrap_or(""));
        doc.add_text(self.fields.content, &chunk.content);
        self.writer.add_document(doc)?;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search_ids(&self, query: &str, top_k: usize) -> Result<Vec<String>> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(
            &self.index,
            vec![self.fields.symbol, self.fields.content, self.fields.path],
        );
        let parsed = parser.parse_query(query)?;
        let docs = searcher.search(&parsed, &TopDocs::with_limit(top_k))?;

        let mut out = Vec::new();
        for (_, address) in docs {
            let doc: TantivyDocument = searcher.doc(address)?;
            if let Some(id_field) = doc.get_first(self.fields.id) {
                let owned = id_field.as_value().as_str().unwrap_or_default().to_string();
                if !owned.is_empty() {
                    out.push(owned);
                }
            }
        }
        Ok(out)
    }
}

fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    let _ = schema_builder.add_text_field("id", STRING | STORED);
    let _ = schema_builder.add_text_field("path", STRING | STORED);
    let _ = schema_builder.add_text_field("symbol", TEXT | STORED);
    let _ = schema_builder.add_text_field("content", TEXT | STORED);
    schema_builder.build()
}

fn from_index(index: Index) -> Result<TantivyLexicalIndex> {
    let schema = index.schema();
    let id = schema.get_field("id")?;
    let path = schema.get_field("path")?;
    let symbol = schema.get_field("symbol")?;
    let content = schema.get_field("content")?;

    let writer = index.writer(50_000_000)?;
    let reader = index.reader()?;
    Ok(TantivyLexicalIndex {
        index,
        reader,
        writer,
        fields: TantivyFields {
            id,
            path,
            symbol,
            content,
        },
    })
}

#[cfg(test)]
mod tests {
    use common::CodeChunk;

    use super::TantivyLexicalIndex;

    #[test]
    fn lexical_index_searches_symbols_and_content() {
        let mut index = TantivyLexicalIndex::new_in_memory().expect("index");
        index
            .add_chunk(&CodeChunk {
                id: "c1".to_string(),
                fingerprint: "fp".to_string(),
                file_path: "src/date.rs".to_string(),
                language: "rust".to_string(),
                symbol: Some("iso_to_date".to_string()),
                start_line: 1,
                end_line: 3,
                start_char: 0,
                end_char: 40,
                content: "fn iso_to_date() -> String { \"x\".to_string() }".to_string(),
            })
            .expect("add");
        index.commit().expect("commit");

        let ids = index.search_ids("iso_to_date", 5).expect("search");
        assert_eq!(ids, vec!["c1".to_string()]);
    }
}
