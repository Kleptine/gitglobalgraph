//! Shared functionality between the git-sync hooks client and server.

use git2::Repository;
use regex::Regex;
use uuid::Uuid;
use hostname::get_hostname;
use std::ops::Deref;
use std::fmt;
use failure::Error;
use failure::ResultExt;
use failure::format_err;
use git2::Reference;
use serde_derive::{Deserialize, Serialize};

pub const GLOBALGRAPH_REPO_NAME: &str = "globalgraph";

// The full commit sha, as a string.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CommitSha(pub String);

impl CommitSha {
    pub fn new(sha: &str) -> CommitSha {
        return CommitSha(sha.to_owned());
    }
}

impl Deref for CommitSha {
    type Target = str;
    fn deref(&self) -> &str {
        return &self.0;
    }
}

impl fmt::Display for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The name of a branch, often called the 'friendly name'.
/// Ex: for the branch full path: [refs/heads/mynamespace/mybranch], the Branch Name
/// would be [mynamespace/mybranch]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BranchName(pub String);

impl BranchName {
    pub fn new(path: &str) -> BranchName {
        return BranchName(path.to_owned());
    }
}

impl Deref for BranchName {
    type Target = str;
    fn deref(&self) -> &str {
        return &self.0;
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// A full reference path. ie. refs/heads/branch_name
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReferencePath(pub String);

impl ReferencePath {
    pub fn new(path: &str) -> ReferencePath {
        return ReferencePath(path.to_owned());
    }
}

impl Deref for ReferencePath {
    type Target = str;
    fn deref(&self) -> &str {
        return &self.0;
    }
}

impl fmt::Display for ReferencePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// The path format that git uses to represent a file in the working directory.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitPath(String);

impl GitPath {
    pub fn new(file_path: &str) -> GitPath {
        return GitPath(file_path.to_owned());
    }
}

impl Deref for GitPath {
    type Target = String;
    fn deref(&self) -> &String {
        return &self.0;
    }
}

impl fmt::Display for GitPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Converts a branch of the format refs/heads/branch_name to the format branch_name
pub fn to_friendly_name(reference_path: &ReferencePath) -> Result<BranchName, Error> {
    let ref_path = Regex::new(r"refs/heads/")?;
    if let Some(_) = ref_path.find(reference_path) {
        return Ok(BranchName(ref_path.replace(reference_path, "").to_string()));
    }
    return Err(format_err!("Branch [{}] was not of the form refs/heads/branch_name", reference_path));
}

/// Represents the head reference of a repository. May be None if the repository does not have a head.
pub type HeadCommit = Option<CommitSha>;


/// Information related to the configuration of a client repository.
#[derive(Debug, PartialEq)]
pub struct ClientSyncConfig {
    pub repo_uuid: String,
}

impl ClientSyncConfig {
    pub fn map_branch_to_global(&self, client_branch: &ReferencePath) -> Result<ReferencePath, Error> {
        let friendly_name = to_friendly_name(client_branch)?;
        Ok(ReferencePath(format!("refs/heads/{}/{}", &self.repo_uuid, friendly_name)))
    }
}

// Takes a global graph branch name and returns the information about where it came from.
pub fn map_branch_to_local(global_branch: &ReferencePath) -> Result<(ClientSyncConfig, ReferencePath), Error> {
    let reg = Regex::new(r"^refs/heads/(?P<repo_uuid>[^/]*)/(?P<branch_name>.*)$")?;
    let captures = reg.captures(global_branch)
        .ok_or(format_err!("The branch [{}] is not a valid Global Graph branch name.", global_branch))?;

    Ok((ClientSyncConfig { repo_uuid: captures["repo_uuid"].to_string() }, ReferencePath(format!("refs/heads/{}", &captures["branch_name"]))))
}

#[derive(Debug, PartialEq, Clone)]
pub struct RepoInformation {
    pub username: String,
    pub machine_name: String,
    pub repo_id: String,
}

/// Breaks apart a UUID string to the components.
pub fn break_repo_uuid(repo_uuid: &str) -> Result<RepoInformation, Error> {
    let reg = Regex::new(r"^(?P<username>[0-9a-z]*)_(?P<machine_name>[0-9a-z]*)_(?P<repo_id>[0-9a-z]*)$")?;
    let captures = reg.captures(repo_uuid)
        .ok_or(format_err!("The string [{}] is not a valid Repository UUID.", repo_uuid))?;

    return Ok(RepoInformation {
        username: captures["username"].into(),
        machine_name: captures["machine_name"].into(),
        repo_id: captures["repo_id"].into(),
    });
}

pub fn generate_repo_id(repo: &Repository) -> Result<String, Error> {
    // It's an error for user.name to be unset.
    let config = repo.config()?;
    let name = config.get_string("user.name")
        .context("Git config user.name not set or invalid. ")?;

    let alpha_num = Regex::new(r"[^a-zA-Z0-9]")?;
    let name_cleaned = alpha_num.replace_all(&name, "").to_lowercase();

    let uuid = &Uuid::new_v4().to_string()[..8];

    let machine_hostname = get_hostname()
        .ok_or_else(|| format_err!("{}", "No machine hostname found. Cannot generate repo_id.".to_owned()))?;

    let repo_id = format!("{}_{}_{}", name_cleaned, machine_hostname, uuid).to_lowercase();

    return Ok(repo_id);
}


// Server structs and request/response json objects.

// TODO(john): Rename to unincorporated changes.
/// A request to modify a set of files at a specific HEAD commit on a local client.
/// Returns whether or not that head incorporates all existing changes to that set
/// of files.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConflictsAfterCommitRequest {
    pub files: Vec<GitPath>,
    /// Either the head commit of the client repository, or None if the client repository has no
    /// valid HEAD. (ie. if it has just been initialized and has no content)
    pub repo_head_commit: HeadCommit,
    pub repo_uuid: String,
}

/// A list of all UnintegratedChanges for a branch.
#[derive(Serialize, Deserialize, Debug)]
pub struct ConflictsAfterCommitResponse {
    /// A list of conflicts that the head didn't incorporate.
    pub conflicts: Vec<UnintegratedChange>,
}

/// Represents a change on a different branch that is not integrated with the
/// target branch. Also known as a 'conflict'.
#[derive(Serialize, Deserialize, Debug)]
pub struct UnintegratedChange {
    pub file: GitPath,
    pub commit: CommitSha,
    pub branch: ReferencePath,
    pub repo_uuid: String,
}

pub trait RepositoryExtensions {
    /// By default, Repository::head will return an Error if no head exists in the repository.
    /// This provides a wrapper that forces you to handle that case.
    fn head_safe(&self) -> Result<Option<Reference>, Error>;
}

impl RepositoryExtensions for Repository {
    fn head_safe(&self) -> Result<Option<Reference>, Error> {
        let head_result = self.head();
        match head_result {
            Ok(reference) => Ok(Some(reference)),
            Err(e) => if e.code() == git2::ErrorCode::UnbornBranch {
                Ok(None)
            } else {
                Err(failure::Error::from(e))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use failure::Error;
    use super::*;

    #[test]
    fn mapping_global_branch_to_local() -> Result<(), Error> {
        assert_eq!(
            map_branch_to_local(&ReferencePath::new("refs/heads/john_desktopmachine_abcdef/mybranch"))?,
            (ClientSyncConfig { repo_uuid: "john_desktopmachine_abcdef".into() }, ReferencePath::new("refs/heads/mybranch")));

        assert_eq!(
            map_branch_to_local(&ReferencePath::new("refs/heads/john_desktopmachine_abcdef/mynamespace/mybranch"))?,
            (ClientSyncConfig { repo_uuid: "john_desktopmachine_abcdef".into() }, ReferencePath::new("refs/heads/mynamespace/mybranch")));

        let result: Result<_, _> = map_branch_to_local(&ReferencePath::new("refs/heads/mybranch"));
        assert!(result.is_err());

        let result: Result<_, _> = map_branch_to_local(&ReferencePath::new("mybranch"));
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn mapping_local_branch_to_global() -> Result<(), Error> {
        let client = ClientSyncConfig {
            repo_uuid: "john_desktopmachine_abcdef".into()
        };

        assert_eq!(
            client.map_branch_to_global(&ReferencePath::new("refs/heads/mybranch"))?,
            ReferencePath::new("refs/heads/john_desktopmachine_abcdef/mybranch"));

        assert_eq!(
            client.map_branch_to_global(&ReferencePath::new("refs/heads/mynamespace/mybranch"))?,
            ReferencePath::new("refs/heads/john_desktopmachine_abcdef/mynamespace/mybranch"));

        let result: Result<_, _> = client.map_branch_to_global(&ReferencePath::new("refs/mybranch"));
        assert!(result.is_err());

        let result: Result<_, _> = client.map_branch_to_global(&ReferencePath::new("mybranch"));
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn break_repo_uuid_tests() -> Result<(), Error> {
        assert!(break_repo_uuid("xx_testusera_desktop_29d519f0").is_err());
        assert!(break_repo_uuid("TestUserA_Desktop_29d519f0").is_err());
        assert!(break_repo_uuid("testusera_desktop").is_err());


        assert_eq!(break_repo_uuid("user12345_desktop12345_someid")?, RepoInformation {
            username: "user12345".into(),
            machine_name: "desktop12345".into(),
            repo_id: "someid".into(),
        });

        assert_eq!(break_repo_uuid("testusera_desktop_29d519f0")?, RepoInformation {
            username: "testusera".into(),
            machine_name: "desktop".into(),
            repo_id: "29d519f0".into(),
        });

        Ok(())
    }
}



