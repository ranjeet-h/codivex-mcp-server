#![allow(deprecated)]

use assert_cmd::Command;
use predicates::str::contains;

fn setup_workspace() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo-a");
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").expect("write");
    (tmp, repo)
}

#[test]
fn add_index_status_remove_flow_works() {
    let (tmp, repo) = setup_workspace();
    let cwd = tmp.path();
    let repo_str = repo.display().to_string();

    Command::cargo_bin("codivex-mcp")
        .expect("binary")
        .current_dir(cwd)
        .args(["add-repo", &repo_str])
        .assert()
        .success()
        .stdout(contains("added repo"));

    Command::cargo_bin("codivex-mcp")
        .expect("binary")
        .current_dir(cwd)
        .args(["index-now"])
        .assert()
        .success()
        .stdout(contains("indexed repo"));

    Command::cargo_bin("codivex-mcp")
        .expect("binary")
        .current_dir(cwd)
        .args(["status"])
        .assert()
        .success()
        .stdout(contains("\"catalog_entries\": 1"))
        .stdout(contains(&repo_str));

    Command::cargo_bin("codivex-mcp")
        .expect("binary")
        .current_dir(cwd)
        .args(["remove-repo", &repo_str])
        .assert()
        .success()
        .stdout(contains("removed repo"));
}
