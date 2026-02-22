use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedChunk {
    pub file: String,
    pub symbol: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedProject {
    pub project_path: String,
    pub files_scanned: usize,
    pub chunks_extracted: usize,
    pub indexed_at_unix: u64,
    pub chunks: Vec<IndexedChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectCatalog {
    pub projects: Vec<ProjectCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCatalogEntry {
    pub project_path: String,
    pub files_scanned: usize,
    pub chunks_extracted: usize,
    pub indexed_at_unix: u64,
}

pub fn read_selected_project(cwd: &Path) -> Option<String> {
    std::fs::read_to_string(selected_project_file(cwd))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn write_selected_project(cwd: &Path, project_path: &str) -> anyhow::Result<()> {
    let target = selected_project_file(cwd);
    assert_state_write_target(cwd, project_path, &target)?;
    std::fs::create_dir_all(codivex_dir(cwd))?;
    std::fs::write(target, project_path)?;
    Ok(())
}

pub fn save_project_index(cwd: &Path, indexed: &IndexedProject) -> anyhow::Result<()> {
    let target = project_index_file(cwd, &indexed.project_path);
    assert_state_write_target(cwd, &indexed.project_path, &target)?;
    let storage_dir = project_storage_dir(cwd, &indexed.project_path);
    assert_state_write_target(cwd, &indexed.project_path, &storage_dir)?;
    std::fs::create_dir_all(project_indexes_dir(cwd))?;
    std::fs::write(target, serde_json::to_string_pretty(indexed)?)?;
    upsert_catalog_entry(cwd, indexed)?;
    Ok(())
}

pub fn load_project_index(cwd: &Path, project_path: &str) -> Option<IndexedProject> {
    let file = project_index_file(cwd, project_path);
    std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str::<IndexedProject>(&raw).ok())
}

pub fn remove_project_index(cwd: &Path, project_path: &str) -> anyhow::Result<()> {
    let index_file = project_index_file(cwd, project_path);
    let storage_dir = project_storage_dir(cwd, project_path);
    assert_state_write_target(cwd, project_path, &index_file)?;
    assert_state_write_target(cwd, project_path, &storage_dir)?;
    let _ = std::fs::remove_file(index_file);
    let _ = std::fs::remove_dir_all(storage_dir);
    let mut catalog = read_catalog(cwd);
    catalog
        .projects
        .retain(|entry| entry.project_path != project_path);
    std::fs::create_dir_all(codivex_dir(cwd))?;
    std::fs::write(
        project_catalog_file(cwd),
        serde_json::to_string_pretty(&catalog)?,
    )?;
    Ok(())
}

pub fn read_catalog(cwd: &Path) -> ProjectCatalog {
    std::fs::read_to_string(project_catalog_file(cwd))
        .ok()
        .and_then(|raw| serde_json::from_str::<ProjectCatalog>(&raw).ok())
        .unwrap_or_default()
}

fn upsert_catalog_entry(cwd: &Path, indexed: &IndexedProject) -> anyhow::Result<()> {
    let mut catalog = read_catalog(cwd);
    if let Some(existing) = catalog
        .projects
        .iter_mut()
        .find(|entry| entry.project_path == indexed.project_path)
    {
        existing.files_scanned = indexed.files_scanned;
        existing.chunks_extracted = indexed.chunks_extracted;
        existing.indexed_at_unix = indexed.indexed_at_unix;
    } else {
        catalog.projects.push(ProjectCatalogEntry {
            project_path: indexed.project_path.clone(),
            files_scanned: indexed.files_scanned,
            chunks_extracted: indexed.chunks_extracted,
            indexed_at_unix: indexed.indexed_at_unix,
        });
        catalog
            .projects
            .sort_by(|a, b| a.project_path.cmp(&b.project_path));
    }
    std::fs::create_dir_all(codivex_dir(cwd))?;
    let target = project_catalog_file(cwd);
    assert_state_write_target(cwd, &indexed.project_path, &target)?;
    std::fs::write(target, serde_json::to_string_pretty(&catalog)?)?;
    Ok(())
}

fn project_key(project_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_path.as_bytes());
    let full = format!("{:x}", hasher.finalize());
    full[..24].to_string()
}

pub fn project_storage_key(project_path: &str) -> String {
    project_key(project_path)
}

pub fn project_storage_dir(cwd: &Path, project_path: &str) -> PathBuf {
    codivex_dir(cwd)
        .join("storage")
        .join(project_key(project_path))
}

pub fn project_lexical_index_dir(cwd: &Path, project_path: &str) -> PathBuf {
    project_storage_dir(cwd, project_path).join("tantivy")
}

pub fn project_vector_collection(project_path: &str) -> String {
    format!("code_chunks_{}", project_key(project_path))
}

fn codivex_dir(cwd: &Path) -> PathBuf {
    cwd.join(".codivex")
}

fn project_indexes_dir(cwd: &Path) -> PathBuf {
    codivex_dir(cwd).join("project-indexes")
}

fn project_index_file(cwd: &Path, project_path: &str) -> PathBuf {
    project_indexes_dir(cwd).join(format!("{}.json", project_key(project_path)))
}

fn selected_project_file(cwd: &Path) -> PathBuf {
    codivex_dir(cwd).join("selected-project.txt")
}

fn project_catalog_file(cwd: &Path) -> PathBuf {
    codivex_dir(cwd).join("project-catalog.json")
}

fn assert_state_write_target(cwd: &Path, project_path: &str, target: &Path) -> anyhow::Result<()> {
    let state_root = codivex_dir(cwd);
    if !target.starts_with(&state_root) {
        anyhow::bail!(
            "unsafe write target outside state directory: {}",
            target.display()
        );
    }
    let repo_root = Path::new(project_path);
    if repo_root.is_absolute() && target.starts_with(repo_root) {
        anyhow::bail!(
            "unsafe write target inside indexed repository: {}",
            target.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        IndexedProject, assert_state_write_target, project_lexical_index_dir, project_storage_dir,
    };

    #[test]
    fn state_write_target_is_rejected_inside_repo_root() {
        let cwd = std::path::PathBuf::from("/tmp/workspace");
        let repo = "/tmp/workspace/repo-a";
        let target = std::path::PathBuf::from("/tmp/workspace/repo-a/.codivex/state.json");
        let err = assert_state_write_target(&cwd, repo, &target).expect_err("must reject");
        assert!(err.to_string().contains("unsafe write target"));
    }

    #[test]
    fn storage_dirs_live_under_codivex_state_root() {
        let cwd = std::path::PathBuf::from("/tmp/workspace");
        let repo = "/tmp/repo-b";
        let storage = project_storage_dir(&cwd, repo);
        let tantivy = project_lexical_index_dir(&cwd, repo);
        assert!(storage.starts_with(cwd.join(".codivex")));
        assert!(tantivy.starts_with(cwd.join(".codivex")));
    }

    #[test]
    fn save_project_index_accepts_safe_state_targets() {
        let cwd = std::env::temp_dir().join(format!("codivex-projects-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).expect("cwd");
        let project = IndexedProject {
            project_path: "/tmp/repo-c".to_string(),
            files_scanned: 1,
            chunks_extracted: 0,
            indexed_at_unix: 1,
            chunks: Vec::new(),
        };
        super::save_project_index(&cwd, &project).expect("save index");
    }
}
