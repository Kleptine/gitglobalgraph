extern crate simple_logger;

#[macro_use]
extern crate log;
extern crate tempfile;
extern crate git2;
extern crate test_utilities;


use test_utilities::*;
use std::error::Error;
use tempfile::Builder;
use std::fs;
use git2::Repository;
use std::process::Command;
use std::path::Path;
use std::path::PathBuf;
use git2::Branch;
use git2::BranchType;
use git2::ObjectType;
use std::env;
use std::sync::{Once, ONCE_INIT};
use std::collections::HashSet;
use std::iter::FromIterator;

use std::collections::hash_map::RandomState;

/// A simple test of the post-commit hook.
/// The global graph should reflect the local commits after commiting.
#[test]
fn test_post_commit_hook() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_hook_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "new text b!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 1);
        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        return Ok(());
    })
}

/// The post commit hook should work fine if the branch reference moves around.
#[test]
fn test_post_commit_hook_reset_head() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_hook_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "text a!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "text b!")])?;

        git_cmd(local_repo, &["reset", "HEAD~1"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./filec.txt"), "text c!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 1);

        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        return Ok(());
    })
}

/// Doing a fast-forward merge should synchronize this client after the reference changes.
#[test]
fn test_merge_commit_sync_fastforward() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_hook_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "text a!")])?;
        git_cmd(local_repo, &["checkout", "-b", "feature"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "text b!")])?;
        git_cmd(local_repo, &["checkout", "master"])?;
        git_cmd(local_repo, &["merge", "feature"])?;

        // Verify that the hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 2);

        // Two commits on master, one on feature.
        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        return Ok(());
    })
}

/// Doing a merge should synchronize this client (after the new commit is created).
#[test]
fn test_merge_commit_sync() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_hook_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "text a!")])?;
        git_cmd(local_repo, &["checkout", "-b", "feature"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "text b!")])?;
        git_cmd(local_repo, &["checkout", "master"])?;
        git_cmd(local_repo, &["merge", "feature", "--no-ff"])?;

        // Verify that the hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 2);

        // Two commits on master, one on feature.
        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        return Ok(());
    })
}

/// Doing a rebase should sync the new graph afterwards.
/// TODO(john): No hooks are run after a rebase, if the rebase fast forwards HEAD.
/// As far as I can tell there's no way to handle this case, other than file watching
/// the branches file.
#[ignore]
#[test]
fn test_rebase_sync() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_hook_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "master a!")])?;
        git_cmd(local_repo, &["checkout", "-b", "feature"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb1.txt"), "feature b1!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb2.txt"), "feature b2!")])?;
        git_cmd(local_repo, &["checkout", "master"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./filec1.txt"), "master c1!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./filec2.txt"), "master c2!")])?;
        git_cmd(local_repo, &["checkout", "feature"])?;

        // Rebase the feature branch onto master.
        git_cmd(local_repo, &["rebase", "master"])?;

        // Verify that the hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 2);

        // 5 total commits after the rebase
        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        return Ok(());
    })
}

/// Simple wrapper to create a couple of temporary repositories to run a test with.
pub fn create_hook_test<F>(test_body: F) -> Result<(), Box<Error>>
    where F: FnOnce(&Repository, &Repository) -> Result<(), Box<Error>>
{
    // Create a directory inside of `std::env::temp_dir()`,
    // whose name will begin with 'example'.
    let test_dir = Builder::new().prefix("sync_server_test").tempdir()?;

    let global_repo_path = test_dir.path().to_owned().join("global");
    let locala_repo_path = test_dir.path().to_owned().join("local_a");
    fs::create_dir(&global_repo_path)?;
    fs::create_dir(&locala_repo_path)?;

    debug!("Creating global repo at {:?}", global_repo_path);
    debug!("Creating a local repo at {:?}", locala_repo_path);

    let global_repo = Repository::init_bare(&global_repo_path)?;
    let locala_repo = Repository::init(&locala_repo_path)?;
    install_all_hooks(&locala_repo)?;

    git_cmd(&locala_repo, &["remote", "add", "sync_server", &global_repo_path.clone().to_string_lossy()])?;
    git_cmd(&locala_repo, &["config", "user.name", "Test User"])?;

    trace!("Starting test.");
    test_body(&locala_repo, &global_repo)
}
