use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use common::{
    CodeChunk,
    projects::{self, IndexedChunk, IndexedProject},
};
use search_core::lexical::TantivyLexicalIndex;

#[derive(Debug, Parser)]
#[command(name = "codivex-mcp")]
#[command(about = "Local MCP code index manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    AddRepo { path: PathBuf },
    RemoveRepo { path: PathBuf },
    ListRepos,
    IndexNow { path: Option<PathBuf> },
    Status,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;

    match cli.command {
        Commands::AddRepo { path } => add_repo(&cwd, &path),
        Commands::RemoveRepo { path } => remove_repo(&cwd, &path),
        Commands::ListRepos => list_repos(&cwd),
        Commands::IndexNow { path } => index_now(&cwd, path.as_deref()),
        Commands::Status => status(&cwd),
    }
}

fn add_repo(cwd: &Path, path: &Path) -> anyhow::Result<()> {
    let repo_path = canonical_repo_path(path)?;
    projects::write_selected_project(cwd, &repo_path)?;
    ensure_catalog_entry(cwd, &repo_path)?;
    println!("added repo: {repo_path}");
    Ok(())
}

fn remove_repo(cwd: &Path, path: &Path) -> anyhow::Result<()> {
    let repo_path = canonical_repo_path(path)?;
    projects::remove_project_index(cwd, &repo_path)?;
    if projects::read_selected_project(cwd).as_deref() == Some(repo_path.as_str()) {
        let _ = projects::write_selected_project(cwd, "");
    }
    println!("removed repo: {repo_path}");
    Ok(())
}

fn list_repos(cwd: &Path) -> anyhow::Result<()> {
    let catalog = projects::read_catalog(cwd);
    for (idx, project) in catalog.projects.iter().enumerate() {
        println!(
            "{}. {} (files={}, chunks={}, indexed_at={})",
            idx + 1,
            project.project_path,
            project.files_scanned,
            project.chunks_extracted,
            project.indexed_at_unix
        );
    }
    Ok(())
}

fn index_now(cwd: &Path, path: Option<&Path>) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => canonical_repo_path(p)?,
        None => projects::read_selected_project(cwd)
            .filter(|v| !v.is_empty())
            .context("no repo selected; pass a path or run add-repo first")?,
    };
    projects::write_selected_project(cwd, &repo_path)?;
    let (files_scanned, chunks_extracted) = run_index(cwd, Path::new(&repo_path))?;
    println!("indexed repo: {repo_path} (files={files_scanned}, chunks={chunks_extracted})");
    Ok(())
}

fn status(cwd: &Path) -> anyhow::Result<()> {
    let selected = projects::read_selected_project(cwd).unwrap_or_default();
    let catalog = projects::read_catalog(cwd);
    let output = serde_json::json!({
        "selected_repo": selected,
        "catalog_entries": catalog.projects.len(),
        "projects": catalog.projects,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn ensure_catalog_entry(cwd: &Path, repo_path: &str) -> anyhow::Result<()> {
    if projects::load_project_index(cwd, repo_path).is_some() {
        return Ok(());
    }
    projects::save_project_index(
        cwd,
        &IndexedProject {
            project_path: repo_path.to_string(),
            files_scanned: 0,
            chunks_extracted: 0,
            indexed_at_unix: unix_now(),
            chunks: Vec::new(),
        },
    )?;
    Ok(())
}

fn run_index(cwd: &Path, repo: &Path) -> anyhow::Result<(usize, usize)> {
    let files = indexer::scanner::scan_source_files(repo);
    let mut indexed_chunks = Vec::new();
    let mut code_chunks = Vec::new();

    for path in &files {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(chunks) =
                indexer::extract_chunks_for_file(path.to_string_lossy().as_ref(), &content)
        {
            for chunk in chunks {
                indexed_chunks.push(IndexedChunk {
                    file: chunk.file_path.clone(),
                    symbol: chunk.symbol.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    content: chunk.content.clone(),
                });
                code_chunks.push(chunk);
            }
        }
    }

    let project_path = repo.display().to_string();
    let indexed = IndexedProject {
        project_path: project_path.clone(),
        files_scanned: files.len(),
        chunks_extracted: indexed_chunks.len(),
        indexed_at_unix: unix_now(),
        chunks: indexed_chunks,
    };
    projects::save_project_index(cwd, &indexed)?;
    persist_tantivy_index(cwd, &project_path, &code_chunks)?;

    Ok((files.len(), indexed.chunks_extracted))
}

fn persist_tantivy_index(
    cwd: &Path,
    project_path: &str,
    chunks: &[CodeChunk],
) -> anyhow::Result<()> {
    let index_dir = projects::project_lexical_index_dir(cwd, project_path);
    let mut index = TantivyLexicalIndex::open_or_create_on_disk(&index_dir)?;
    index.reset()?;
    for chunk in chunks {
        index.add_chunk(chunk)?;
    }
    index.commit()?;
    Ok(())
}

fn canonical_repo_path(path: &Path) -> anyhow::Result<String> {
    let canonical = std::fs::canonicalize(path).with_context(|| {
        format!(
            "repo path does not exist or is not accessible: {}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        anyhow::bail!("repo path is not a directory: {}", canonical.display());
    }
    Ok(canonical.display().to_string())
}

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}
