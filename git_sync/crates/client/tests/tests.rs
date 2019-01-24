use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use failure::format_err;
use git2::Branch;
use git2::BranchType;
use git2::Repository;
use log::{debug, error, trace};
use tempfile::Builder;

use client::init_logging;
use std::thread;

// Note: These tests no longer work. The integration tests are the reliable tests.
// These were from an older time when it made sense to test the client without a running server.

/// A simple test of the post-commit hook.
/// The global graph should reflect the local commits after commiting.
#[test]
fn test_post_commit_hook() -> Result<(), failure::Error> {
    init_logging();

    create_repo_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "new text b!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 1);
        assert_eq!(global_repo.total_commits()?, 2);

        return Ok(());
    })
}

/// The post commit hook should work fine if the branch reference moves around.
#[test]
fn test_post_commit_hook_reset_head() -> Result<(), failure::Error> {
    init_logging();

    create_repo_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "text a!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "text b!")])?;

        git_cmd(local_repo, &["reset", "HEAD~1"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./filec.txt"), "text c!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 1);
        assert_eq!(global_repo.total_commits()?, 2);

        return Ok(());
    })
}

/// Committing files that aren't 'lockable' in the git attributes should work like normal.
#[test]
fn test_post_merge_hook() -> Result<(), failure::Error> {
    init_logging();

    create_repo_test(|local_repo, global_repo| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "text a!")])?;
        git_cmd(local_repo, &["checkout", "-b", "feature"])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "text b!")])?;
        git_cmd(local_repo, &["checkout", "master"])?;
        git_cmd(local_repo, &["merge", "feature", "--no-ff"])?;

        // Verify that the hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 2);

        // Two commits on master, one on feature.
        assert_eq!(global_repo.total_commits()?, 3);

        return Ok(());
    })
}

fn create_repo_test<F>(test_body: F) -> Result<(), failure::Error>
    where F: FnOnce(&Repository, &Repository) -> Result<(), failure::Error>
{
    // Create a directory inside of `std::env::temp_dir()`,
    // whose name will begin with 'example'.
    let test_dir = Builder::new().prefix(shared::GLOBALGRAPH_REPO_NAME).tempdir()?;

    let global_repo_path = test_dir.path().to_owned().join("global");
    let locala_repo_path = test_dir.path().to_owned().join("local_a");
    fs::create_dir(&global_repo_path)?;
    fs::create_dir(&locala_repo_path)?;

    debug!("Creating global repo at {:?}", global_repo_path);
    debug!("Creating a local repo at {:?}", locala_repo_path);

    let global_repo = Repository::init_bare(&global_repo_path)?;
    let locala_repo = Repository::init(&locala_repo_path)?;
    install_all_hooks(&locala_repo)?;

    git_cmd(&locala_repo, &["remote", "add", shared::GLOBALGRAPH_REPO_NAME, &global_repo_path.clone().to_string_lossy()])?;
    git_cmd(&locala_repo, &["config", "user.name", "Test User"])?;

    test_body(&locala_repo, &global_repo)
}


trait RepositoryExtensions {
    /// Gets the total number of commits reachable from references in the repository.
    fn total_commits(&self) -> Result<usize, git2::Error>;
}

impl RepositoryExtensions for Repository {
    fn total_commits(&self) -> Result<usize, git2::Error> {
        return self.branches(None)?.map(|b: Result<(Branch, BranchType), git2::Error>| -> Result<usize, git2::Error> {
            let branch = b?.0;
            let mut revwalk = self.revwalk()?;
            revwalk.push_ref(branch.into_reference().name().unwrap())?;
            Ok(revwalk.count())
        }).try_fold(0, |acc, c| -> Result<usize, git2::Error> {
            Ok(acc + c?)
        });
    }
}

#[cfg(windows)]
fn install_all_hooks(repo: &Repository) -> Result<(), failure::Error> {
    debug!("Installing hooks to [{:?}]", repo.path());

    let hooks_dir = repo.path().join(r"hooks\");
    install_hook(&hooks_dir, "pre-commit")?;
    install_hook(&hooks_dir, "post-commit")?;

    Ok(())
}

#[cfg(windows)]
fn install_hook<P: AsRef<Path>>(hooks_dir: P, hook_name: &str) -> Result<(), failure::Error> {
    let hooks_dir = hooks_dir.as_ref();

    if !hooks_dir.exists() {
        fs::create_dir(&hooks_dir)?;
    }

    let hook_src = std::env::current_dir().unwrap().join(format!("..\\..\\target\\debug\\{}.exe", hook_name));

    let hook_dst = hooks_dir.join(format!("{}.exe", hook_name));

    trace!("Copying [{:?}] to [{:?}]", &hook_src, &hook_dst);

    fs::copy(&hook_src, &hook_dst)
        .map_err(|e| format_err!("Couldn't install [[{}] hook. Message: [{}]", hook_name, e.description()))?;

    Ok(())
}



/// Runs a git command with the embedded git binary. For testing only.
#[must_use]
fn git_cmd(repo: &Repository, arguments: &[&str]) -> Result<(), failure::Error> {
    // Locate the proper git command
    let cmd = git_cmd_path();
    let mut cmd_to_run = Command::new(&cmd);
    cmd_to_run
        .args(arguments)
        .current_dir(&repo.workdir().unwrap());

    debug!("Running git command: {:?}", cmd_to_run);
    let output = cmd_to_run.output()?;

    trace!("STDOUT:\n{}", String::from_utf8_lossy(&output.stdout));
    trace!("STDERR:\n{}", String::from_utf8_lossy(&output.stderr));

    if output.status.success() {
        return Ok(());
    } else {
        error!("Git command [{:?}] failed with stderr:\n {}", arguments, String::from_utf8_lossy(&output.stderr));
        return Err(format_err!("Git command [{:?}] failed with message: {}", arguments, String::from_utf8_lossy(&output.stderr)));
    }
}

/// Returns the path to the embedded git executable. Just for running tests.
#[cfg(windows)]
fn git_cmd_path() -> PathBuf {
    return PathBuf::from(env!("CARGO_MANIFEST_DIR").to_owned()).join(r".\..\..\git_bin\win\bin\git.exe");
}

fn change_and_commit<P: AsRef<Path>>(repo: &Repository, changes: &[(P, &str)]) -> Result<(), failure::Error> {
    for &(ref file, ref text) in changes {
        let filepath = repo.workdir().unwrap().join(file);
        fs::write(filepath, text)?;
    }

    let files = changes.iter()
        .map(|&(ref file, _)| file.as_ref().to_string_lossy().into_owned()).collect::<Vec<String>>();

    let files: Vec<&str> = files.iter().map(|f| &**f).collect();

    let mut arguments = vec!("add");
    arguments.extend(files);
    git_cmd(repo, &arguments[..])?;

    git_cmd(repo, &["commit", "-m", "Make some changes."])?;

    Ok(())
}
