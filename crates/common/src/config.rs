use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub repo_paths: Vec<String>,
    pub ignore_paths: Vec<String>,
    pub model_path: String,
    pub default_top_k: usize,
    pub enable_metrics: bool,
    pub api_token: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            repo_paths: vec!["/repo".to_string()],
            ignore_paths: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
            ],
            model_path: "models/all-minilm-l6-v2.onnx".to_string(),
            default_top_k: 20,
            enable_metrics: true,
            api_token: None,
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let mut cfg = if path.exists() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("failed reading config file: {}", path.display()))?;
            toml::from_str::<Self>(&raw)
                .with_context(|| format!("failed parsing config file: {}", path.display()))?
        } else {
            Self::default()
        };

        if let Ok(paths) = std::env::var("CODEVIX_REPO_PATHS") {
            cfg.repo_paths = paths
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
        if let Ok(model) = std::env::var("CODEVIX_MODEL_PATH") {
            cfg.model_path = model;
        }
        if let Ok(top_k) = std::env::var("CODEVIX_DEFAULT_TOP_K") {
            cfg.default_top_k = top_k.parse().unwrap_or(cfg.default_top_k);
        }
        if let Ok(token) = std::env::var("MCP_API_TOKEN") {
            cfg.api_token = Some(token);
        }

        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::AppConfig;

    #[test]
    fn loads_default_when_file_missing() {
        let cfg = AppConfig::load(PathBuf::from("does-not-exist.toml").as_path()).expect("config");
        assert_eq!(cfg.default_top_k, 20);
    }

    #[test]
    fn loads_toml_file() {
        let mut path = std::env::temp_dir();
        path.push("codivex-config-test.toml");
        fs::write(
            &path,
            "repo_paths=['/tmp/repo']\nignore_paths=['.git']\nmodel_path='m.onnx'\ndefault_top_k=7\nenable_metrics=true\n",
        )
        .expect("write");

        let cfg = AppConfig::load(path.as_path()).expect("config");
        assert_eq!(cfg.repo_paths, vec!["/tmp/repo".to_string()]);
        assert_eq!(cfg.default_top_k, 7);
    }
}
