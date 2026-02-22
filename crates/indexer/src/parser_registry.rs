use anyhow::{Result, anyhow};
use tree_sitter::{Language, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageKind {
    Rust,
    C,
    Cpp,
    JavaScript,
    TypeScript,
    Python,
    Go,
    Haskell,
    Java,
    CSharp,
    Php,
    Ruby,
    Kotlin,
    Swift,
}

impl LanguageKind {
    pub fn from_path(path: &str) -> Option<Self> {
        let lower = path.to_ascii_lowercase();
        for kind in Self::all() {
            if kind.extensions().iter().any(|ext| lower.ends_with(ext)) {
                return Some(*kind);
            }
        }
        None
    }

    pub const fn all() -> &'static [Self] {
        &[
            Self::Rust,
            Self::C,
            Self::Cpp,
            Self::JavaScript,
            Self::TypeScript,
            Self::Python,
            Self::Go,
            Self::Haskell,
            Self::Java,
            Self::CSharp,
            Self::Php,
            Self::Ruby,
            Self::Kotlin,
            Self::Swift,
        ]
    }

    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[".rs"],
            Self::C => &[".c", ".h"],
            Self::Cpp => &[
                ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx", ".ipp", ".tpp", ".inl",
            ],
            Self::JavaScript => &[".js", ".jsx", ".mjs", ".cjs"],
            Self::TypeScript => &[".ts", ".tsx"],
            Self::Python => &[".py", ".pyi"],
            Self::Go => &[".go"],
            Self::Haskell => &[".hs", ".lhs"],
            Self::Java => &[".java"],
            Self::CSharp => &[".cs"],
            Self::Php => &[".php", ".phtml"],
            Self::Ruby => &[".rb"],
            Self::Kotlin => &[".kt", ".kts"],
            Self::Swift => &[".swift"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Go => "go",
            Self::Haskell => "haskell",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Kotlin => "kotlin",
            Self::Swift => "swift",
        }
    }
}

#[derive(Default)]
pub struct ParserRegistry;

impl ParserRegistry {
    pub fn new() -> Self {
        Self
    }

    pub fn parser_for_path(&self, path: &str) -> Result<(LanguageKind, Parser)> {
        let language_kind =
            LanguageKind::from_path(path).ok_or_else(|| anyhow!("unsupported file extension"))?;
        let language = language_for(language_kind);

        let mut parser = Parser::new();
        parser.set_language(&language)?;
        Ok((language_kind, parser))
    }
}

fn language_for(kind: LanguageKind) -> Language {
    match kind {
        LanguageKind::Rust => tree_sitter_rust::LANGUAGE.into(),
        LanguageKind::C => tree_sitter_c::LANGUAGE.into(),
        LanguageKind::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        LanguageKind::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        LanguageKind::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        LanguageKind::Python => tree_sitter_python::LANGUAGE.into(),
        LanguageKind::Go => tree_sitter_go::LANGUAGE.into(),
        LanguageKind::Haskell => tree_sitter_haskell::LANGUAGE.into(),
        LanguageKind::Java => tree_sitter_java::LANGUAGE.into(),
        LanguageKind::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        LanguageKind::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        LanguageKind::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        LanguageKind::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        LanguageKind::Swift => tree_sitter_swift::LANGUAGE.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{LanguageKind, ParserRegistry};

    #[test]
    fn detects_language_from_path() {
        assert_eq!(
            LanguageKind::from_path("src/main.rs"),
            Some(LanguageKind::Rust)
        );
        assert_eq!(LanguageKind::from_path("foo.c"), Some(LanguageKind::C));
        assert_eq!(LanguageKind::from_path("foo.cpp"), Some(LanguageKind::Cpp));
        assert_eq!(
            LanguageKind::from_path("foo.py"),
            Some(LanguageKind::Python)
        );
        assert_eq!(
            LanguageKind::from_path("App.hs"),
            Some(LanguageKind::Haskell)
        );
        assert_eq!(
            LanguageKind::from_path("Foo.java"),
            Some(LanguageKind::Java)
        );
        assert_eq!(
            LanguageKind::from_path("Foo.cs"),
            Some(LanguageKind::CSharp)
        );
        assert_eq!(
            LanguageKind::from_path("index.php"),
            Some(LanguageKind::Php)
        );
        assert_eq!(LanguageKind::from_path("app.rb"), Some(LanguageKind::Ruby));
        assert_eq!(
            LanguageKind::from_path("Main.kt"),
            Some(LanguageKind::Kotlin)
        );
        assert_eq!(
            LanguageKind::from_path("AppDelegate.swift"),
            Some(LanguageKind::Swift)
        );
        assert_eq!(LanguageKind::from_path("foo.unknown"), None);
    }

    #[test]
    fn creates_parser_for_supported_extension() {
        let registry = ParserRegistry::new();
        let result = registry.parser_for_path("src/main.rs");
        assert!(result.is_ok());
    }
}
