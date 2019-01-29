use tempfile::Builder;
use std::fs;
use git2::Repository;
use std::process::Command;
use std::path::Path;
use std::path::PathBuf;
use git2::ObjectType;
use std::sync::{Once, ONCE_INIT};
use std::collections::HashSet;
use log::{error, trace, debug};
use log::Level;
use failure::Fail;

use actix_web::{test};

use failure::Error;
use failure::ResultExt;
use std::process::Output;
use std::fmt;

static INIT_LOGGING: Once = ONCE_INIT;

pub fn init_logging() {
    INIT_LOGGING.call_once(|| {
        simple_logger::init_with_level(Level::Info).unwrap();
    });
}

pub struct TestHarness<'a> {
    pub local_repo_a: &'a Repository,
    pub local_repo_b: &'a Repository,
    pub origin_repo: &'a Repository,
    pub global_graph: &'a Repository,
    pub server: &'a mut test::TestServer,
}

/// Simple wrapper to create a couple of temporary repositories to run a test with.
pub fn create_integration_test<F>(test_body: F) -> Result<(), Error>
    where F: FnOnce(TestHarness) -> Result<(), Error>
{
    // Create a directory inside of `std::env::temp_dir()`,
    // whose name will begin with 'example'.
    let test_dir = Builder::new().prefix(shared::GLOBALGRAPH_REPO_NAME).tempdir()?;

    // Create a work directory for the server
    let server_work_dir = test_dir.path().to_owned().join("server");
    fs::create_dir(&server_work_dir)?;

    let locala_repo_path = test_dir.path().to_owned().join("local_a");
    let localb_repo_path = test_dir.path().to_owned().join("local_b");
    let origin_repo_path = test_dir.path().to_owned().join("origin");
    fs::create_dir(&locala_repo_path)?;
    fs::create_dir(&localb_repo_path)?;
    fs::create_dir(&origin_repo_path)?;

    debug!("Creating Global Graph server with working directory: {:?}", server_work_dir);
    let origin_repo = Repository::init_bare(&origin_repo_path)?;

    debug!("Starting global graph server.");
    let mut srv = test::TestServer::with_factory(server::create_server_factory(&server_work_dir)?);
    let server_url = srv.url("");

    debug!("Cloning a Local Repo A at {:?}", locala_repo_path);
    let locala_repo = Repository::init(&locala_repo_path)?;
    install_all_hooks(&locala_repo)?;

    let global_repo_path = server_work_dir.join("repo");
    let global_repo_url = PathBuf::from("file://".to_string()).join(server_work_dir.join("repo"));
    let origin_repo_url = PathBuf::from("file://".to_string()).join(&origin_repo_path);
    git_cmd(&locala_repo, &["remote", "add", shared::GLOBALGRAPH_REPO_NAME, &global_repo_url.clone().to_string_lossy()])?;
    git_cmd(&locala_repo, &["remote", "add", "origin", &origin_repo_url.clone().to_string_lossy()])?;
    git_cmd(&locala_repo, &["config", "user.name", "Test User A"])?;
    git_cmd(&locala_repo, &["config", "globalgraph.server", &server_url])?;

    debug!("Creating an origin repo at {:?}", &origin_repo_path);
    let global_repo = Repository::init_bare(&global_repo_path)?;

    debug!("Setting up initial files and commits in the repository.");
    change_and_commit(&locala_repo, &[(&PathBuf::from("./Readme.md"), "Initial commit")])?;
    change_and_commit(&locala_repo, &[(&PathBuf::from("./.gitattributes"),
        "*.bin lockable"
    )])?;
    debug!("Pushing initial commits to origin.");
    git_cmd(&locala_repo, &["push", "origin", "master"])?;

    debug!("Cloning a local repo B from origin at {:?}", localb_repo_path);
    let localb_repo = Repository::clone(&origin_repo_url.to_string_lossy(), &localb_repo_path)?;
    install_all_hooks(&localb_repo)?;

    git_cmd(&localb_repo, &["remote", "add", shared::GLOBALGRAPH_REPO_NAME, &global_repo_url.clone().to_string_lossy()])?;
    git_cmd(&localb_repo, &["config", "user.name", "Test User B"])?;
    git_cmd(&localb_repo, &["config", "globalgraph.server", &server_url])?;

    trace!("Starting test.");
    test_body(TestHarness {
        local_repo_a: &locala_repo,
        local_repo_b: &localb_repo,
        origin_repo: &origin_repo,
        global_graph: &global_repo,
        server: &mut srv,
    })
}


pub trait RepositoryTestExtensions {
    /// Gets the total number of commits in the repository.
    fn total_commits(&self) -> Result<usize, git2::Error>;

    /// Gets the total number of commits reachable from references in the repository.
    fn total_reachable_commits(&self) -> Result<usize, git2::Error>;

    fn all_commits(&self) -> Result<HashSet<git2::Oid>, git2::Error>;
}


impl RepositoryTestExtensions for Repository {
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

    fn all_commits(&self) -> Result<HashSet<git2::Oid>, git2::Error> {
        let odb = self.odb()?;
        // NOTE(john): This is not performant enough for large repos.
        let mut commits: HashSet<git2::Oid> = HashSet::new();

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
}

/// Installs all git-sync hooks to a git repository for a test.
#[cfg(windows)]
pub fn install_all_hooks(repo: &Repository) -> Result<(), Error> {
    debug!("Installing hooks to [{:?}]", repo.path());

    let hooks_dir = repo.path().join(r"hooks\");
    install_hook(&hooks_dir, "pre-commit")?;
    install_hook(&hooks_dir, "post-commit")?;
    install_hook(&hooks_dir, "post-rewrite")?;
    install_hook(&hooks_dir, "post-merge")?;

    Ok(())
}

#[cfg(windows)]
pub fn install_hook<P: AsRef<Path>>(hooks_dir: P, hook_name: &str) -> Result<(), Error> {
    let hooks_dir = hooks_dir.as_ref();

    if !hooks_dir.exists() {
        fs::create_dir(&hooks_dir)?;
    }

    let hook_src = std::env::current_dir().unwrap().join(format!("../../target/debug/{}.exe", hook_name));
    let hook_dst = hooks_dir.join(format!("{}.exe", hook_name));

    trace!("Copying [{:?}] to [{:?}]", &hook_src, &hook_dst);
    fs::copy(&hook_src, &hook_dst)
        .context(format!("Couldn't install [[{}] hook. ", hook_name))?;

    Ok(())
}

#[derive(Fail, Debug)]
pub struct CommandError {
    pub output: Output,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "A sub-command failed with the status code [{}]. stdout: \n[{}], stderr: \n[{}]",
                          self.output.status,
                          String::from_utf8_lossy(&self.output.stdout),
                          String::from_utf8_lossy(&self.output.stderr))
    }
}

/// Runs a git command with the embedded git binary. For testing only.
#[must_use]
pub fn git_cmd(repo: &Repository, arguments: &[&str]) -> Result<(), CommandError> {
    // Locate the proper git command
    let cmd = git_cmd_path();

    let mut cmd_to_run = Command::new(&cmd);
    cmd_to_run
        .args(arguments)
        .current_dir(&repo.workdir().unwrap());

    debug!("Running git command: {:?}", cmd_to_run);
    let output = cmd_to_run.output()
        .expect("Couldn't find git cli executable. Needed to run tests.");

    // stdout and stderr are the places where logging from the postcommit hooks come from.
    if !&output.stdout.is_empty() {
        trace!("Output (STDOUT):\n{}", String::from_utf8_lossy(&output.stdout));
    }
    if !&output.stderr.is_empty() {
        trace!("Output (STDERR):\n{}", String::from_utf8_lossy(&output.stderr));
    }

    if output.status.success() {
        return Ok(());
    } else {
        error!("Git command [{:?}] failed.", arguments);
        error!("Git command stdout:\n{}", String::from_utf8_lossy(&output.stdout));
        error!("::::::::::::::::::::::::::::::::::");
        error!("Git command stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        error!("::::::::::::::::::::::::::::::::::");
        return Err(CommandError { output: output.clone() });
    }
}

/// Returns the path to the embedded git executable. Just for running tests.
#[cfg(windows)]
fn git_cmd_path() -> PathBuf {
    return PathBuf::from(env!("CARGO_MANIFEST_DIR").to_owned()).join(r"..\..\git_bin\win\bin\git.exe");
}

pub fn change_and_commit<P: AsRef<Path>>(repo: &Repository, changes: &[(P, &str)]) -> Result<(), Error> {
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
