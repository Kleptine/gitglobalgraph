#[macro_use]
extern crate log;

extern crate env_logger;
extern crate git2;
extern crate regex;
extern crate uuid;
extern crate hostname;

use git2::Repository;
use git2::BranchType;
use git2::ErrorCode;
use std::error::Error;
use std::sync::{Once, ONCE_INIT};
use log::LevelFilter;
use env_logger::Builder;
use std::path::Path;
use regex::Regex;
use uuid::Uuid;
use hostname::get_hostname;

struct ClientSyncConfig {
    repo_uuid: String,
}

/// Given a path to a repository on the local file system, synchronizes this repository as a
/// client on the sync server.
///
/// Every branch `b` in the local repository will be mapped to a branch like `developer_repo/b`.
pub fn synchronize_local_repository<P: AsRef<Path>>(repository_path: P) -> Result<(), Box<Error>> {
    init_logging();

    // First push all branches
    let repo = Repository::open(repository_path)?;
    info!("Loaded repo at: {:?}", repo.path());

    // If we don't have a repo uuid, set it.
    let uuid_current = repo.config()?.get_string("sync.repouuid").map(|v| v.to_owned());

    match uuid_current {
        Ok(_uuid) => {}
        Err(e) => {
            match e.code() {
                ErrorCode::NotFound => {
                    let new_uuid = generate_repo_id(&repo)?;
                    trace!("Failed to get UUID: Error: {:?}", e.code());
                    info!("No UUID set for this repository. Setting name to [{}]", &new_uuid);
                    repo.config()?.set_str("sync.repouuid", &new_uuid)?;
                }
                _ => {
                    return Err(Box::from(e));
                }
            }
        }
    }

    let uuid = repo.config()?.get_string("sync.repouuid")?;
    debug!("Syncing repo [{}] to the global server.", uuid);

    // Verify the repository is setup for git-sync
    let config = get_config(&repo)
        .map_err(|e| "The local repository is improperly configured. \n".to_owned() + e.description())?;

    let branches = repo.branches(None)?
        .filter_map(Result::ok)
        .filter(|&(_, t)| t == BranchType::Local)
        .map(|(branch, _type)| branch);

    for branch in branches {
        let reference = branch.into_reference();
        debug!("Syncing branch: {:?}", reference.name());

        let branch_short_name = str::replace(reference.name().unwrap(), "refs/heads/", "");

        // Always force push the branch to the sync server, as we are the only user.
        let refspec = format!("+{}:{}",
                              reference.name().unwrap(),
                              map_client_branch_name_to_global(&config, &branch_short_name));

        debug!("Pushing refspec: {}", &refspec);

        let _ = repo.find_remote("sync_server")?.push(&[&refspec], None)?;
    }

    Ok(())

    // Then push synchronize the index file.
    // TODO(john)
}

fn get_config(repo: &Repository) -> Result<ClientSyncConfig, Box<Error>> {
    let repo_uuid = repo.config()?.get_string("sync.repouuid")?;

    if repo_uuid.is_empty() {
        return Err(Box::from(String::from("Test")));
    }

    Ok(ClientSyncConfig {
        repo_uuid: repo_uuid,
    })
}


static INIT_LOGGING: Once = ONCE_INIT;

pub fn init_logging() {
    INIT_LOGGING.call_once(|| {
        let env = env_logger::Env::default()
            .filter_or(env_logger::DEFAULT_FILTER_ENV, "debug");

        env_logger::Builder::from_env(env)
            .init();
    })
}

pub fn generate_repo_id(repo: &Repository) -> Result<String, Box<Error>> {
    // It's an error for user.name to be unset.
    let config = repo.config()?;
    let name = config.get_string("user.name")
        .map_err(|e| "git config user.name not set or invalid. ".to_owned() + e.description())?;


    let alpha_num = Regex::new(r"[^a-zA-Z0-9]")?;
    let name_cleaned = alpha_num.replace_all(&name, "").to_lowercase();

    let uuid = &Uuid::new_v4().to_string()[..8];

    let machine_hostname = get_hostname().ok_or_else(|| "No machine hostname found. Cannot generate repo_id.".to_owned())?;

    let repo_id = format!("{}_{}_{}", name_cleaned, machine_hostname, uuid).to_lowercase();

    return Ok(repo_id);
}

fn map_client_branch_name_to_global(local_repo_config: &ClientSyncConfig, friendly_branch_name: &str) -> String {
    return format!("refs/heads/{}/{}", local_repo_config.repo_uuid, friendly_branch_name);
}