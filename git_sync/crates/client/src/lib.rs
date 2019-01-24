use git2::Repository;
use git2::BranchType;
use git2::ErrorCode;
use std::sync::{Once, ONCE_INIT};
use std::path::Path;
use shared::ClientSyncConfig;
use shared::ReferencePath;
use failure::Error;
use failure::ResultExt;
use log::{debug, trace, info};
use failure::format_err;
use log::Level;

/// Given a path to a repository on the local file system, synchronizes this repository as a
/// client on the sync server.
///
/// Every branch `b` in the local repository will be mapped to a branch like `developer_repo/b`.
pub fn synchronize_local_repository<P: AsRef<Path>>(repository_path: P) -> Result<(), Error> {
    init_logging();

    // First push all branches
    let repo = Repository::open(repository_path)?;
    debug!("Loaded repo at: {:?}", repo.path());

    // If we don't have a repo uuid, set it.
    let uuid = get_or_create_client_uuid(&repo)?;

    debug!("Syncing repo [{}] to the global server.", uuid);
    debug!("Global Graph remote url: [{}]", repo.find_remote(shared::GLOBALGRAPH_REPO_NAME)?.url().ok_or(format_err!("The global graph remote url is not valid UTF8."))?);

    // Verify the repository is setup for git-sync
    let config = get_config(&repo)
        .context("The local repository is improperly configured.")?;

    let branches = repo.branches(None)?
        .filter_map(Result::ok)
        .filter(|&(_, t)| t == BranchType::Local)
        .map(|(branch, _type)| branch);

    for branch in branches {
        let reference = branch.into_reference();
        debug!("Syncing branch: {:?}", reference.name());

        // Always force push the branch to the sync server, as we are the only user.
        let branch_name = ReferencePath::new(reference.name().unwrap());
        let refspec = format!("+{}:{}",
                              branch_name,
                              config.map_branch_to_global(&branch_name)?);

        debug!("Pushing refspec: {}", &refspec);

        let _ = repo.find_remote(shared::GLOBALGRAPH_REPO_NAME)?.push(&[&refspec], None)?;
    }

    Ok(())

    // Then push synchronize the index file.
    // TODO(john)
}

/// Gets the UUID associated with the given repository, or if it isn't set, generates one and sets
/// it. Returns Error if setting or reading of the config failed.
pub fn get_or_create_client_uuid(repo: &Repository) -> Result<String, Error> {
    let uuid_current = repo.config()?.get_string("globalgraph.repouuid").map(|v| v.to_owned());

    match uuid_current {
        Ok(uuid) => {
            return Ok(uuid);
        }
        Err(e) => {
            match e.code() {
                ErrorCode::NotFound => {
                    let new_uuid = shared::generate_repo_id(&repo)?;

                    trace!("Failed to get UUID: Error: {:?}", e.code());
                    info!("No UUID set for this repository. Setting uuid to [{}]", &new_uuid);
                    repo.config()?.set_str("globalgraph.repouuid", &new_uuid)?;
                    return Ok(new_uuid);
                }
                _ => {
                    return Err(Error::from(e));
                }
            }
        }
    }
}

fn get_config(repo: &Repository) -> Result<ClientSyncConfig, Error> {
    let repo_uuid = repo.config()?.get_string("globalgraph.repouuid")?;

    if repo_uuid.is_empty() {
        return Err(format_err!("Test"));
    }

    Ok(ClientSyncConfig {
        repo_uuid,
    })
}


static INIT_LOGGING: Once = ONCE_INIT;

pub fn init_logging() {
    INIT_LOGGING.call_once(|| {
        simple_logger::init_with_level(Level::Trace).unwrap();
    })
}
