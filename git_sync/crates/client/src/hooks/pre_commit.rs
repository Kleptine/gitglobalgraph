
#[macro_use]
extern crate log;
extern crate client;

use std::error::Error;
use std::env;

/// precommit hook entry point
fn main() -> Result<(), Box<Error>>{
    client::init_logging();

    debug!("Starting pre-commit.");
    debug!("Pre-commit finished.");

    Ok(())

}
