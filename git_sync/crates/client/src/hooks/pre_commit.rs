#[macro_use]
extern crate log;
extern crate client;
#[macro_use]
extern crate failure;
extern crate reqwest;
extern crate url;
extern crate git2;
extern crate serde_json;
extern crate shared;
extern crate http;

use std::env;
use url::Url;

use failure::Error;
use failure::ResultExt;
use git2::Repository;
use shared::CommitSha;
use shared::RepositoryExtensions;
use http::StatusCode;
use git2::Status;
use git2::CheckAttributeFlags;
use git2::AttributeType;
use shared::GitPath;

/// precommit hook entry point
/// Returns 3 potential values:
///    Ok(true) -- everything is successful.
///    Ok(false) -- the hook executed successfully, but conflicts were found.
///    Err(_) -- the hook failed to execute, conflicts not checked.
fn main_internal() -> Result<bool, Error> {
    client::init_logging();

    debug!("Starting pre-commit.");

    debug!("Synchronizing the repository.");
    client::synchronize_local_repository(env::current_dir()?)
        .context("Synchronization with server failed. \n")?;

    info!("[Global Graph]: Checking for conflicts in the Global Graph.");
    let repo = Repository::open(env::current_dir()?)?;
    let global_graph_url = repo.config()?.get_string("globalgraph.server")
        .context("The local git config value 'globalgraph.server' is missing or invalid. \
        Set it to your global graph query server url.")?;
    let host_url = Url::parse(&global_graph_url)
        .context(format!("The local git config value 'globalgraph.server' is not a valid url: [{}]", &global_graph_url))?;

    let find_unintegrated_changes_url = host_url.join("v1/conflicts_after_commit")?;

    // Make a request to the server to check if we can commit the changed files.

    let repo_uuid = repo.config()?.get_string("globalgraph.repouuid")
        .context("The local git config value 'globalgraph.repouuid' is missing or invalid.")?;


    // TODO(john): Get the file names by running: git status --porcelain

    // TODO (john): Verify a number of cases:
    //      - Renamed files should check both the source and destination paths for conflicts.
    //      - Symlinks work as expected, and only trigger conflicts if the link changes (not what it points at)
    //      - Unchanged, but touched files shouldn't show up here.

    // Get a list of the files the client has changed.
    let mut modified_paths: Vec<GitPath> = vec!();

    for entry in repo.statuses(None)?
        .iter()
        .filter(|entry| (Status::INDEX_NEW | Status::INDEX_MODIFIED | Status::INDEX_DELETED | Status::INDEX_RENAMED).contains(entry.status()))
        {
            if let Some(path) = entry.path() {
                let result = repo.get_attr(CheckAttributeFlags::empty(), path, "lockable").unwrap();
                if result == AttributeType::True {
                    modified_paths.push(GitPath::new(path))
                }
            } else {
                warn!("[Global Graph] Warning: Path is not valid UTF8 and will be ignored for conflict checks. [{}](lossy)", String::from_utf8_lossy(entry.path_bytes()));
            }
        }

    debug!("Modified Paths: [{:#?}]", modified_paths);

    let payload = shared::ConflictsAfterCommitRequest {
        repo_uuid: repo_uuid,
        repo_head_commit: match repo.head_safe()? {
            Some(reference) => Some(CommitSha(reference.peel_to_commit()?.id().to_string())),
            None => None
        },
        files: modified_paths,
    };
    let client = reqwest::Client::new();
    let mut response = client.post(find_unintegrated_changes_url.as_str())
        .json(&payload)
        .send().context("Could not send request to the Global Graph server.")?;

    if response.status() != StatusCode::OK {
        return Err(format_err!("The Global Graph server returned a non-200 status code: [{}].", response.status()));
    }

    trace!("Global Graph conflicts check returned status code: [{}]", response.status());
    let response_payload: shared::ConflictsAfterCommitResponse = response.json()
        .context("The Global Graph server returned invalid json.")?;

    if !response_payload.conflicts.is_empty() {
        error!("[Global Graph]: Found one or more conflicting commits on other branches:");
        for conflict in response_payload.conflicts {
            let username = match shared::break_repo_uuid(&conflict.repo_uuid) {
                Ok(info) => info.username,
                Err(_) => {
                    error!("    Note: Couldn't parse the conflicting Repository name: [{}]", &conflict.repo_uuid);
                    "<Unknown User>".to_string()
                }
            };

            error!("    Local file [{}] is in conflict with another version of the file, committed by user [{}]. Conflicting version:", conflict.file, username);
            error!("      Repository UUID: [{}]", conflict.repo_uuid);
            error!("      Branch: [{}]", conflict.branch);
            error!("      Commit: [{}]", conflict.commit);

            return Ok(false);
        }
    } else {
        info!("[Global Graph]: No conflicts found. Clear to commit.");
    }

    debug!("Pre-commit finished.");

    Ok(true)
}

// The hook only returns a 0 error code if there are no conflicts. 
fn main() -> Result<(), Error> {
    let result = main_internal();
    match result {
        Ok(true) => std::process::exit(0),

        // Either of these cases stops the git commit operation.
        Ok(false) => {
            error!("Conflicts found. Exiting with status: [2].");
            std::process::exit(2);
        }
        Err(e) => {
            // TODO(john): Implement GG_CONFLICTS_IGNORE_ONCE
            error!("An unrecoverable error occurred when checking this commit for conflicts on the Global Graph. This may mean the local repository is configured incorrectly, the server is unreachable, or the server returned an invalid response.\nIf you want to force a commit (and potentially put this repo in conflict with another commit, set the environment flag: \"GG_CONFLICTS_IGNORE_ONCE=1\")");
            return Err(e);
        }
    }
}
