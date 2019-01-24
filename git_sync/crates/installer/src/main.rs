//! The installer for the Global Graph client. Standalone as an executable.
//!
//! The executable files for the installed git commit hooks are included in this installer via rust's `include_bytes`.

use failure::Error;
use structopt::StructOpt;
use std::path::PathBuf;
use git2::Repository;
use failure::ResultExt;
use std::fs;
use std::io;
use std::io::BufRead;
use regex::Regex;
use log::{info};
use failure::format_err;

/// The options provided on the command line.
#[derive(StructOpt, Debug)]
#[structopt(name = "globalgraph-configure")]
struct Opt {
    /// The URL of the Global Graph Git repo.
    ///
    /// example: https://server.com/globalgraph.git
    ///
    /// example: git@server.com:repository/respository.git
    #[structopt(long = "global_graph_repo")]
    global_graph_url: String,

    /// The URL of the Global Graph query server.
    ///
    /// example: https://server.com:12345
    ///
    /// example: https://192.168.1.30:12345
    #[structopt(long = "query_server_url")]
    server_url: String,

    /// The path to the git directory you would like to configure. This git repository will be
    /// configured to synchronize its history to the global_graph_repo specified alongside this command.
    ///
    /// example: ./
    ///
    /// example: C:\Users\Me\Documents\MyRepository
    ///
    /// example: /home/me/my_repository
    ///
    #[structopt(long = "git_path", parse(from_os_str), default_value = "./")]
    git_directory: PathBuf,

    /// Whether or not to enable conflicts detection in this repository.
    /// Conflicts detection installs an additional set of hooks that will check all
    /// commits made locally for conflicts in the global graph.
    /// For more information, see http://todo.com
    #[structopt(long = "conflicts_detection")]
    conflicts_detection: bool,
}

/// Entry point. Installs the GlobalGraph client to the computer.
fn main() -> Result<(), Error> {
    let env = env_logger::Env::default()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, "info");

    env_logger::Builder::from_env(env).init();

    let args = Opt::from_args();

    let repo = Repository::open(&args.git_directory)
        .context(format!("The argument git_path [{}] was not a valid git repository.", args.git_directory.to_string_lossy()))?;

    if repo.is_bare() {
        return Err(format_err!("The git repository [{}] is a bare repository, and can't act as a client in the Global Graph.
        If you are trying to start the server component of the Global Graph, this is the wrong executable.", args.git_directory.to_string_lossy()));
    }

    match repo.config()?.get_string("globalGraph.installed") {
        Ok(_) => {
            println!("WARNING: Global Graph is already installed in this git repository. This will reconfigure the settings for this repository. Continue? (y|n)");
            if !get_yes_or_no()? {
                println!("Exiting.");
                return Ok(());
            }
        }
        Err(_) => {}
    }

    install_hook(&repo, "post-commit", include_bytes!("../../../target/debug/post-commit.exe"))?;

    if args.conflicts_detection {
        // Add conflicts detection.
        install_hook(&repo, "pre-commit", include_bytes!("../../../target/debug/pre-commit.exe"))?;
    } else {
        // TODO(john): Remove conflicts detection.
    }

    info!("All hooks updated.");

    info!("Verifying global graph repository URL: [{}]", args.global_graph_url);

    repo.config()
        .context("Error when accessing the configuration store for this Git repo. Could not mark repository as 'installed'.")?
        .set_str("globalGraph.installed", "installed")
        .context("Could not mark repository as 'installed'.")?;

    println!("Installation finished successfully with the following configuration: {:#?}", args);
    Ok(())
}

fn install_hook(repo: &Repository, hook_name: &str, hook_bytes: &[u8]) -> Result<(), Error> {
    info!("Installing hook [{}]", hook_name);

    // Add hooks under hookname.d/
    let hookd_directory_name = format!("{}.d/", hook_name);
    let hookd_directory = repo.path().join("hooks").join(&hookd_directory_name);

    info!("Making sure directory [{:?}] exists.", hookd_directory);
    fs::create_dir_all(&hookd_directory)?;

    // TODO(john): Support moving git hooks named 'hook.exe'
    let hook_path = repo.path().join("hooks").join(hook_name);

    // Move old hook to hookname.d/
    if hook_path.exists() {
        // If the existing main hook is the global graph dispatcher hook, then we don't want to copy it.
        let hook_contents_result = fs::read_to_string(&hook_path);
        let move_old_hook = if let Ok(hook_contents) = hook_contents_result {
            // If the hook is already the dispatcher hook, so just update the global graph hook in hook.d/
            !is_hook_dispatcher(&hook_contents)?
        } else {
            false
        };

        if move_old_hook {
            let new_hook_path = hookd_directory.join(hook_name);
            info!("Moving old hook from [{:?}] to [{:?}].", hook_path, new_hook_path);
            if new_hook_path.exists() {
                return Err(format_err!("Tried to move existing [{}] hook to path [{}], but a file already existed.", hook_name, new_hook_path.to_string_lossy()));
            }
            fs::rename(&hook_path, &new_hook_path)?;
        } else {
            info!("Hook [{}] is already set up as a dispatching hook. No need to make it a dispatcher.", hook_name)
        }
    }

    // Copy global graph hook to directory
    let globalgraph_hook_path = hookd_directory.join("globalgraph");

    info!("Writing Global Graph hook to [{:?}].", globalgraph_hook_path);
    fs::write(globalgraph_hook_path, hook_bytes.as_ref())?;

    // Add sh files that executes all hooks in the hook.d directory.
    info!("Writing new hook to [{:?}] that executes all the hooks in [{}] directory.", hook_path, hookd_directory_name);
    fs::write(hook_path, format!(
"#HOOK_DISPATCH
for file in ./{}.d/*; do $file; done", hook_name))?;

    Ok(())
}

/// Checks the contents of a hook file to see if this hook is the global graph hook dispatcher.
fn is_hook_dispatcher(hook_contents: &str) -> Result<bool, Error> {
    // Check the string contents of the file to see if has the global graph signifier.
    let global_graph_signifier = Regex::new(r"^#HOOK_DISPATCH(\r\n|\r|\n)")?;
    if global_graph_signifier.find(hook_contents).is_some() {
        return Ok(true);
    } else {
        return Ok(false);
    }
}

fn get_yes_or_no() -> Result<bool, Error> {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        match line?.as_ref() {
            "Y" | "y" | "yes" | "Yes" | "YES" => return Ok(true),
            "N" | "n" | "no" | "No" | "NO" => return Ok(false),
            _ => {}
        }
    }

    return Ok(false);
}
