use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct QualityDataset {
    version: String,
    project_path: String,
    queries: Vec<QualityQuery>,
}

#[derive(Debug, Deserialize)]
struct QualityQuery {
    query: String,
    expected_file_substring: String,
}

#[derive(Debug, Serialize)]
struct QualityReport {
    dataset_version: String,
    project_path: String,
    total_queries: usize,
    mrr_at_10: f64,
    recall_at_5: f64,
    hits_at_1: usize,
    hits_at_5: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dataset_path = std::env::var("QUALITY_DATASET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("benchmarks/quality-dataset-v1.json"));
    let output_path = std::env::var("QUALITY_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("benchmarks/quality-report.json"));

    let dataset: QualityDataset = serde_json::from_str(
        &std::fs::read_to_string(&dataset_path)
            .with_context(|| format!("failed reading {}", dataset_path.display()))?,
    )
    .with_context(|| format!("failed parsing {}", dataset_path.display()))?;
    let cwd = std::env::current_dir()?;
    let project_path = resolve_project_path(&cwd, &dataset.project_path);

    let mut reciprocal_rank_sum = 0.0;
    let mut hits_at_1 = 0usize;
    let mut hits_at_5 = 0usize;
    for q in &dataset.queries {
        let items =
            mcp_server::services::search::scoped_project_results(&cwd, &project_path, &q.query, 10)
                .await
                .unwrap_or_default();
        let rank = items
            .iter()
            .position(|item| item.file.contains(&q.expected_file_substring));
        if let Some(idx) = rank {
            reciprocal_rank_sum += 1.0 / ((idx + 1) as f64);
            if idx == 0 {
                hits_at_1 += 1;
            }
            if idx < 5 {
                hits_at_5 += 1;
            }
        }
    }

    let total = dataset.queries.len().max(1);
    let report = QualityReport {
        dataset_version: dataset.version,
        project_path: redact_path(&project_path),
        total_queries: dataset.queries.len(),
        mrr_at_10: reciprocal_rank_sum / (total as f64),
        recall_at_5: (hits_at_5 as f64) / (total as f64),
        hits_at_1,
        hits_at_5,
    };
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, serde_json::to_string_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn resolve_project_path(cwd: &std::path::Path, value: &str) -> String {
    if let Ok(override_path) = std::env::var("QUALITY_PROJECT_PATH")
        && !override_path.trim().is_empty()
    {
        return override_path;
    }
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "." {
        return cwd.display().to_string();
    }
    let p = std::path::Path::new(trimmed);
    if p.is_absolute() {
        return p.display().to_string();
    }
    cwd.join(p).display().to_string()
}

fn redact_path(path: &str) -> String {
    let redact = std::env::var("QUALITY_REDACT_PATH")
        .map(|v| !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    if !redact {
        return path.to_string();
    }
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|name| format!("<redacted:{name}>"))
        .unwrap_or_else(|| "<redacted>".to_string())
}
