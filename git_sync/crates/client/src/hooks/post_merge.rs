
#[macro_use]
extern crate log;
extern crate client;

use std::error::Error;
use std::env;

/// precommit hook entry point
fn main() -> Result<(), Box<Error>>{
    client::init_logging();

    debug!("Starting post-merge.");
    client::synchronize_local_repository(env::current_dir()?)
        .expect("Synchronization with server failed. \n");
    debug!("Finished post-merge.");

    Ok(())

}
