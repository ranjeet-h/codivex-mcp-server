use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::parser_registry::LanguageKind;

const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

pub fn scan_source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let max_file_bytes = max_file_bytes_from_env();
    let walker = WalkBuilder::new(root).hidden(false).build();
    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        if !is_supported_source_file(path) {
            continue;
        }
        if is_ignored_path(path) {
            continue;
        }
        if let Ok(meta) = path.metadata() {
            if meta.len() > max_file_bytes {
                continue;
            }
        }
        if is_probably_binary(path) {
            continue;
        }
        out.push(path.to_path_buf());
    }
    out
}

fn is_ignored_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("/.git/")
        || path_str.contains("/node_modules/")
        || path_str.contains("/target/")
        || path_str.contains("/build/")
        || path_str.contains("/dist/")
        || path_str.contains("/.next/")
        || path_str.contains("/.turbo/")
        || path_str.contains("/.pnpm-store/")
        || path_str.contains("/.yarn/cache/")
        || path_str.contains("/.cache/")
        || path_str.contains("/coverage/")
        || path_str.contains("/vendor/bundle/")
        || path_str.contains("/Pods/")
        || path_str.contains("/DerivedData/")
        || path_str.contains("/.gradle/")
        || path_str.contains("/out/")
        || path_str.contains("/bin/")
        || path_str.contains("/obj/")
}

fn is_supported_source_file(path: &Path) -> bool {
    LanguageKind::from_path(&path.to_string_lossy()).is_some()
}

fn is_probably_binary(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.iter().take(512).any(|b| *b == 0)
}

fn max_file_bytes_from_env() -> u64 {
    std::env::var("INDEX_MAX_FILE_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_FILE_BYTES)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{max_file_bytes_from_env, scan_source_files};

    #[test]
    fn scanner_skips_ignored_dirs() {
        let base = std::env::temp_dir().join("codivex-scan-test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("src")).expect("mkdir src");
        fs::create_dir_all(base.join("node_modules/pkg")).expect("mkdir nm");
        fs::write(base.join("src/main.rs"), "fn main() {}").expect("write src");
        fs::write(base.join("node_modules/pkg/a.js"), "x").expect("write nm");

        let files = scan_source_files(Path::new(&base));
        let joined = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(joined.iter().any(|p| p.ends_with("src/main.rs")));
        assert!(!joined.iter().any(|p| p.contains("node_modules")));
    }

    #[test]
    fn scanner_only_includes_supported_extensions() {
        let base = std::env::temp_dir().join("codivex-scan-ext-test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("src")).expect("mkdir src");
        fs::write(base.join("src/main.rs"), "fn main() {}").expect("write rust");
        fs::write(base.join("src/README.md"), "# doc").expect("write md");

        let files = scan_source_files(Path::new(&base));
        let joined = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(joined.iter().any(|p| p.ends_with("src/main.rs")));
        assert!(!joined.iter().any(|p| p.ends_with("README.md")));
    }

    #[test]
    fn max_file_bytes_uses_env_override() {
        let prev = std::env::var("INDEX_MAX_FILE_BYTES").ok();
        // SAFETY: tests in this crate only read this env var and this test restores it.
        unsafe {
            std::env::set_var("INDEX_MAX_FILE_BYTES", "1234");
        }
        assert_eq!(max_file_bytes_from_env(), 1234);
        match prev {
            Some(v) => {
                // SAFETY: restoring original process env value for the same key.
                unsafe { std::env::set_var("INDEX_MAX_FILE_BYTES", v) }
            }
            None => {
                // SAFETY: clearing test env override.
                unsafe { std::env::remove_var("INDEX_MAX_FILE_BYTES") }
            }
        }
    }
}
