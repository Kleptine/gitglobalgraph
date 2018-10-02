#[macro_use]
extern crate log;

extern crate actix;
extern crate actix_web;
extern crate bytes;
extern crate env_logger;
extern crate futures;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate json;
extern crate git2;

use actix_web::{
    error, http, middleware, server, App, AsyncResponder, HttpMessage,
    HttpRequest, HttpResponse, Json,
};

use bytes::BytesMut;
use futures::{Future, Stream};
use json::JsonValue;
use std::path::Path;
use std::ops::Deref;
use git2::Branch;
use git2::Repository;
use std::path::PathBuf;
use std::error::Error;
use git2::BranchType;

// The full commit sha, as a string.
#[derive(Debug, Serialize, Deserialize)]
struct CommitSha(String);

impl Deref for CommitSha {
    type Target = String;
    fn deref(&self) -> &String {
        return &self.0;
    }
}

// A full reference path. ie. refs/heads/branch_name
#[derive(Debug, Serialize, Deserialize)]
struct ReferenceName(String);

impl Deref for ReferenceName {
    type Target = String;
    fn deref(&self) -> &String {
        return &self.0;
    }
}

// The path format that git uses to represent a file in the working directory.
#[derive(Debug, Serialize, Deserialize)]
struct GitPath(String);

impl Deref for GitPath {
    type Target = String;
    fn deref(&self) -> &String {
        return &self.0;
    }
}

pub struct AppState {
    work_directory: PathBuf,
}


// TODO(john): Rename to unincorporated changes.
/// A request to modify a set of files at a specific HEAD commit on a local client.
/// Returns whether or not that head incorporates all existing changes to that set
/// of files.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConflictsAfterCommitRequest {
    files: Vec<GitPath>,
    repo_head: ReferenceName,
    repo_uuid: String,
}

/// Represents the conflict for a specific file modification.
#[derive(Serialize, Deserialize, Debug)]
pub struct BinaryConflict {
    file: GitPath,
    commit: CommitSha,
    user: String,
    repo_uuid: String,
    branch: ReferenceName,
}

/// Information about why the server rejected a file modification request.
#[derive(Serialize, Deserialize, Debug)]
pub struct ConflictsAfterCommitResponse {
    /// A list of conflicts that the head didn't incorporate.
    pub conflicts: Vec<BinaryConflict>,
}

/// Returns all branches in the global graph that can conflict with the given branch.
fn get_conflicting_branches<'repo>(global_graph: &'repo Repository, target_branch: &Branch) -> Result<Vec<Branch<'repo>>, Box<Error>> {
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
    // TODO(john): This will be updated after divergence is added.
}

/// This handler uses json extractor
fn conflicts_after_commit(request: &HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    request.json().from_err()
        .and_then(|payload: ConflictsAfterCommitRequest| {
            info!("Request: {:?}", payload);
            let response = ConflictsAfterCommitResponse {
                conflicts: vec!()
            };
//            Ok(HttpResponse::Ok().json(response)) // <- send response
            Err("String Error")
        }).responder()
}

pub fn create_server_factory(work_directory: &PathBuf) ->
impl Fn() -> App<AppState>
{
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

    return server_app_factory;
}


fn main() {
    ::std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();
    let sys = actix::System::new("json-example");

    server::new(create_server_factory(&::std::env::current_dir().unwrap()))
        .bind("127.0.0.1:8080")
        .unwrap()
        .shutdown_timeout(1)
        .start();

    println!("Started http server: 127.0.0.1:8080");

    let _ = sys.run();
}

