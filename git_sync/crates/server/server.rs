use actix_web::{
    http, middleware, server, App, AsyncResponder, HttpMessage,
    HttpRequest, HttpResponse
};

use log::{debug, info};
use futures::{Future};
use git2::Branch;
use git2::Repository;
use std::path::PathBuf;
use git2::BranchType;
use failure::Error;
use failure::ResultExt;
use failure::err_msg;
use shared::*;
use git2::Commit;
use git2::Oid;
use structopt::StructOpt;
use failure::format_err;


pub struct AppState {
    work_directory: PathBuf,
}

// Returns information about the Global Graph server that is running.
pub struct InfoResponse {
    pub global_graph_git_remote_url: String,
}

/// Returns all branches in the global graph that can conflict with the given branch.
/// TODO(john): Currently all branches conflict with all other branches. Waiting on divergence.
fn get_conflicting_branches<'repo>(global_graph: &'repo Repository, _target_head: &HeadCommit) -> Result<Vec<Branch<'repo>>, Error> {
    let all_branches = global_graph.branches(None)?
        .filter(|branch_result| match branch_result {
            Ok((_, t)) => *t == BranchType::Local,
            Err(_) => true,
        })
        .map(|branch_result| match branch_result {
            Ok((branch, _type)) => Ok(branch),
            Err(e) => Err(e),
        });
    let branch_vec = all_branches.collect::<Result<Vec<Branch<'repo>>, git2::Error>>()?;
    return Ok(branch_vec);
}

/// Given a target_branch and some changes, determines whether these changes can be committed on
/// this branch. If not, it returns a reasoning. 
fn check_integration(repo: &Repository, commit_head: &HeadCommit, files: &[GitPath]) -> Result<Vec<UnintegratedChange>, Error> {
    let branches = get_conflicting_branches(&repo, &commit_head)?;
    let mut unintegrated_changes = vec!();
    let commit_head_object = match commit_head {
        Some(commit) => Some(repo.find_commit(Oid::from_str(commit)?)?),
        None => None
    };

    // For every branch that can conflict with the client's branch, check to make sure
    // the client has integrated its changes, for the files specified.
    for conflict_branch in branches {
        debug!("Checking branch [{}]", conflict_branch.name()?.ok_or(format_err!("Conflict candidate branch has a name that is not valid UTF8."))?);

        for file in files {
            debug!("   - checking file [{:?}]", file);

            // Find the most recent commit that touches this file on the conflict branch.
            let mut revwalk = repo.revwalk()?;
            revwalk.push_ref(&conflict_branch.get().name()
                .ok_or(err_msg("A branch name in the Global Graph is invalid UTF-8"))?)?;

            let mut latest_commit: Option<Commit> = None;
            'find_latest_commit: for oid in revwalk {
                let commit = repo.find_commit(oid?)?;
                for tree_entry in commit.tree()?.iter() {
                    // Skip file paths in the tree that aren't valid UTF-8
                    if let Some(file_name) = tree_entry.name() {
                        if file_name == **file {
                            latest_commit = Some(commit);
                            break 'find_latest_commit;
                        }
                    }
                }
            }

            match latest_commit {
                // This file has never been modified on the conflicting branch, so the branch doesn't conflict.
                None => { continue; }

                // Verify that the current head integrates this change.
                Some(latest_commit) => {
                    let does_integrate = match commit_head_object {
                        Some(ref head_object) => repo.graph_descendant_of(head_object.id().clone(), latest_commit.id())?,
                        // If the client's repository has no valid head, then it definitely does not integrate
                        // the changed file.
                        None => false
                    };
                    debug!("Found latest commit [{:?}]. Does the target branch integrate it: [{}]", latest_commit, does_integrate);

                    if does_integrate {
                        // The target head already integrated this change, so we don't have a conflict.
                        continue;
                    } else {
                        let conflicting_branch_name = ReferencePath::new(conflict_branch.get().name()
                            .ok_or(format_err!("Conflict found, but conflicting branch name was not UTF8."))?);
                        let (client_info, local_branch_reference) = map_branch_to_local(&conflicting_branch_name)?;
//                        let repo_info = break_repo_uuid(&client_info.repo_uuid)?;

                        unintegrated_changes.push(UnintegratedChange {
                            file: (*file).clone(),
                            commit: CommitSha::new(&format!("{}", latest_commit.id())),
                            branch: local_branch_reference,
                            repo_uuid: client_info.repo_uuid,
                        });
                    }
                }
            }
        }
    }

    Ok(unintegrated_changes)
}

/// Handles requests made to check whether conflicts would occur after a commit is made at a give head.
fn conflicts_after_commit(request: &HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    let work_dir = request.state().work_directory.clone();
    request.json().from_err()
        .and_then(move |payload: ConflictsAfterCommitRequest| {
            debug!("Received request: {:?}", payload);

            let repo_path = work_dir.join("repo");
            let repo = Repository::open_bare(repo_path).context("Could not open global graph repository path.").compat()?;

            // Verify the local client is properly synced. It's HEAD and Branch should be the same
            // in the global graph.
//            let _client = ClientSyncConfig {
//                repo_uuid: payload.repo_uuid.clone(),
//            };
//            let gg_branch = client.map_branch_to_global(&payload.repo_branch)?;
//            let target_branch = repo.find_branch(&to_friendly_name(&gg_branch)?, BranchType::Local)
//                .context(format!("The client's current branch [{:?}] was not found in the Global Graph.", &gg_branch)).compat()?;

            let unintegrated_changes = check_integration(&repo, &payload.repo_head_commit, &payload.files)?;
            let response = ConflictsAfterCommitResponse {
                conflicts: unintegrated_changes
            };

            Ok(HttpResponse::Ok().json(response)) // <- send response
        }).responder()
}

fn prepare_work_directory(work_directory: &PathBuf) -> Result<(), Error> {
    if !work_directory.exists() {
        return Err(format_err!("Working directory path does not exist: {:?}", work_directory));
    }

    let repo_path = work_directory.join("repo");

    if let Err(git_error) = Repository::open(&repo_path) {
        match git_error.code() {
            git2::ErrorCode::NotFound => {
                info!("Global Graph repository does not exist under the working directory. Initializing new repository at {:?}", &repo_path);
                Repository::init_bare(&repo_path)?;
            }
            _ => {
                return Err(Error::from(git_error));
            }
        }
    }

    // TODO(john): Assert that the directory is either empty or contains only the required files.

    Ok(())
}

pub fn create_server_factory(work_directory: &PathBuf) ->
Result<impl Fn() -> App<AppState>, Error>
{
    prepare_work_directory(work_directory)?;

    let workdir = work_directory.clone();
    let server_app_factory = move || {
        return App::with_state(AppState {
            work_directory: PathBuf::from(workdir.clone())
        })
            // enable logger
            .middleware(middleware::Logger::default())
            .resource("/v1/conflicts_after_commit", |r| {
                r.method(http::Method::POST).f(conflicts_after_commit)
            });
    };

    return Ok(server_app_factory);
}

#[derive(StructOpt, Debug)]
struct Opt {
    /// The address to bind the Global Graph Query server to.
    ///
    /// ex: 127.0.0.1:8080
    #[structopt(long="bind_address", default_value="127.0.0.1:8080")]
    bind_address: String,

    /// The working directory to store the Global Graph repo, local data, and other
    /// temporary files.
    ///
    /// ex: ./   ./workdir/
    #[structopt(long="work_dir", parse(from_os_str))]
    work_directory: PathBuf,
}

/// The main entry point of the server executable.
/// This function may be dead if this crate is being used as a library, rather than a binary.
#[allow(dead_code)]
fn main() -> Result<(), Error> {
    env_logger::init();

    let args = Opt::from_args();

    info!("Working Directory: [{:?}]", &args.work_directory);

    if !args.work_directory.exists() {
        return Err(format_err!("The working directory specified for the Global Graph does not exist: [{:?}]", &args.work_directory));
    }

    // Setup the global graph repo if it doesn't already exist.
    let global_graph_repo = args.work_directory.join("repo");
    if global_graph_repo.exists() {
        info!("Loading existing Global Graph repository: [{:?}].", &global_graph_repo.to_string_lossy());
        let repo = Repository::open(&global_graph_repo)
            .context(format!("The path [{}] was not a valid git repository.", global_graph_repo.to_string_lossy()))?;

        if !repo.is_bare() {
            return Err(format_err!("The Global Graph repository [{}] must be a bare repository.", global_graph_repo.to_string_lossy()));
        }
    } else {
        info!("No existing Global Graph repo found, creating new one at [{:?}].", &global_graph_repo);
        Repository::init_bare(&global_graph_repo)?;
    }

    let sys = actix::System::new("global-graph-server");
    server::new(create_server_factory(&::std::env::current_dir().unwrap())?)
        .bind(&args.bind_address)?
        .shutdown_timeout(5)
        .start();

    println!("############################################################");
    println!("  The Global Graph repo is at: [{:?}]", global_graph_repo);
    println!("  Started the Global Graph query server at: [{}]", &args.bind_address);
    println!("############################################################");

    let _ = sys.run();
    Ok(())
}

