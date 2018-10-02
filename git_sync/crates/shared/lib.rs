//! Shared functionality between the git-sync hooks client and server.

#[macro_use]
extern crate log;

extern crate env_logger;
extern crate git2;
extern crate regex;
extern crate uuid;
extern crate hostname;


pub mod sync_server {
    use git2::Repository;
    use git2::BranchType;
    use git2::ErrorCode;
    use regex::Regex;
    use uuid::Uuid;
    use hostname::get_hostname;
    use std::error::Error;

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
}


