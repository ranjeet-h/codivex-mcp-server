pub mod chunking;
pub mod dedup;
pub mod fingerprint;
pub mod incremental;
pub mod parser_registry;
pub mod scanner;
pub mod symbol_map;
pub mod sync;
pub mod telemetry;
pub mod watcher;
pub mod worker;

pub use chunking::extract_chunks_for_file;
pub use parser_registry::{LanguageKind, ParserRegistry};
pub use symbol_map::SymbolMap;
