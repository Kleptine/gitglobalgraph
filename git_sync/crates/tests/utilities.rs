extern crate simple_logger;

#[macro_use]
extern crate log;
extern crate tempfile;
extern crate git2;
extern crate actix_web;
extern crate server;

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

use actix_web::{test, http};

static INIT_LOGGING: Once = ONCE_INIT;
pub fn init_logging() -> Result<(), log::SetLoggerError> {
    INIT_LOGGING.call_once(|| {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    });

    Ok(())
}


/// Simple wrapper to create a couple of temporary repositories to run a test with.
pub fn create_integration_test<F>(test_body: F) -> Result<(), Box<Error>>
    where F: FnOnce(&Repository, &Repository, &mut test::TestServer) -> Result<(), Box<Error>>
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

    debug!("Starting global graph server.");
    let mut srv = test::TestServer::with_factory(server::create_server_factory(&PathBuf::from("")));

    trace!("Starting test.");
    test_body(&locala_repo, &global_repo, &mut srv)
}


pub trait RepositoryExtensions {

    /// Gets the total number of commits in the repository.
    fn total_commits(&self) -> Result<usize, git2::Error>;

    /// Gets the total number of commits reachable from references in the repository.
    fn total_reachable_commits(&self) -> Result<usize, git2::Error>;

    fn all_commits(&self) -> Result<HashSet<git2::Oid>, git2::Error>;
}


impl RepositoryExtensions for Repository {
    fn all_commits(&self) -> Result<HashSet<git2::Oid>, git2::Error> {
        let odb = self.odb()?;
        // NOTE(john): This is not performant enough for large repos.
        let mut commits : HashSet<git2::Oid> = HashSet::new();

        odb.foreach(|oid| -> bool {
            let object = odb.read(oid.clone());
            match object {
                Ok(object) => {
                    match object.kind() {
                        ObjectType::Commit => {
                            commits.insert(oid.clone());
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("{}", e);
                    return false;
                }
            }

            return true;
        })?;

        return Ok(commits);

    }
    fn total_commits(&self) -> Result<usize, git2::Error> {
        let mut count: usize = 0;
        let odb = self.odb()?;
        odb.foreach(|oid| -> bool {
            let object = odb.read(oid.clone());
            match object {
                Ok(object) => {
                    match object.kind() {
                        ObjectType::Commit => {
                            count += 1;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("{}", e);
                    return false;
                }
            }

            return true;
        })?;

        return Ok(count);
    }

    fn total_reachable_commits(&self) -> Result<usize, git2::Error> {
        let branches = self.branches(None)?;

        let mut revwalk = self.revwalk()?;

        for branch in branches {
            let (branch, _) = branch?;
            revwalk.push_ref(branch.into_reference().name().unwrap())?;
        }

        return Ok(revwalk.into_iter().count());
    }
}

/// Installs all git-sync hooks to a git repository for a test.
#[cfg(windows)]
pub fn install_all_hooks(repo: &Repository) -> Result<(), Box<Error>> {
    debug!("Installing hooks to [{:?}]", repo.path());

    let hooks_dir = repo.path().join(r"hooks\");
    install_hook(&hooks_dir, "pre-commit")?;
    install_hook(&hooks_dir, "post-commit")?;
    install_hook(&hooks_dir, "post-rewrite")?;
    install_hook(&hooks_dir, "post-merge")?;

    Ok(())
}

#[cfg(windows)]
pub fn install_hook<P: AsRef<Path>>(hooks_dir: P, hook_name: &str) -> Result<(), Box<Error>> {
    let hooks_dir = hooks_dir.as_ref();

    if !hooks_dir.exists() {
        fs::create_dir(&hooks_dir)?;
    }

    let hook_src = std::env::current_dir().unwrap().join(format!("../../target/debug/{}.exe", hook_name));
    let hook_dst = hooks_dir.join(format!("{}.exe", hook_name));

    trace!("Copying [{:?}] to [{:?}]", &hook_src, &hook_dst);
    fs::copy(&hook_src, &hook_dst)
        .map_err(|e| format!("Couldn't install [[{}] hook. ", hook_name) + e.description())?;

    Ok(())
}

/// Runs a git command with the embedded git binary. For testing only.
#[must_use]
pub fn git_cmd(repo: &Repository, arguments: &[&str]) -> Result<(), Box<Error>> {
    // Locate the proper git command
    let cmd = git_cmd_path();

    let mut cmd_to_run = Command::new(&cmd);
    cmd_to_run
        .args(arguments)
        .current_dir(&repo.workdir().unwrap());

    debug!("Running git command: {:?}", cmd_to_run);
    let output = cmd_to_run.output()
        .expect("Couldn't find git cli executable. Needed to run tests.");

    trace!("STDOUT:\n{}", String::from_utf8_lossy(&output.stdout));
    if !&output.stderr.is_empty() {
        debug!("STDERR:\n{}", String::from_utf8_lossy(&output.stderr));
    }

    if output.status.success() {
        return Ok(());
    } else {
        error!("Git command [{:?}] failed with stderr:\n {}", arguments, String::from_utf8_lossy(&output.stderr));
        return Err(Box::from(format!("Git command [{:?}] failed with message: {}", arguments, String::from_utf8_lossy(&output.stderr))));
    }
}

/// Returns the path to the embedded git executable. Just for running tests.
#[cfg(windows)]
fn git_cmd_path() -> PathBuf {
    return PathBuf::from(env!("CARGO_MANIFEST_DIR").to_owned()).join(r"..\..\git_bin\win\bin\git.exe");
}

pub fn change_and_commit<P: AsRef<Path>>(repo: &Repository, changes: &[(P, &str)]) -> Result<(), Box<Error>> {
    for &(ref file, ref text) in changes {
        let filepath = repo.workdir().unwrap().join(file);
        trace!("Making a change to file [{:?}] with contents [{:?}]", filepath, text);
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
