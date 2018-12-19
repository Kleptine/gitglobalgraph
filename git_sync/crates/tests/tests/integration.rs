extern crate test_utilities;
extern crate actix_web;
extern crate serde_json;
extern crate server;
extern crate client;

extern crate failure;

#[macro_use]
extern crate json;
extern crate shared;
extern crate git2;

use test_utilities::*;
use std::path::PathBuf;
use actix_web::http;
use http::StatusCode;
use actix_web::{HttpRequest, HttpMessage};
use actix_web::test::TestServer;
use std::str;
use json::JsonValue;
use serde_json::value::Value;
use shared::GitPath;
use shared::ReferencePath;
use shared::CommitSha;

use failure::Error;
use failure::ResultExt;
use test_utilities::CommandError;
use git2::BranchType;

/// After committing the global graph should be properly synchronized.
#[test]
fn single_push() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        change_and_commit(harness.local_repo_a, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(harness.local_repo_a, &[(&PathBuf::from("./fileb.txt"), "new text b!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(harness.global_graph.branches(None)?.count(), 1);
        assert_eq!(harness.local_repo_a.all_commits(), harness.global_graph.all_commits());

        return Ok(());
    })
}

/// The local repo b is cloned from origin. On a successful clone, when the global graph is configured,
/// we should have synchronized and have a repouuid for local repo b.
#[test]
fn synchronize_after_clone() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./testfile.txt"), "new test")])?;

        let uuid = harness.local_repo_b.config()?.get_string("globalgraph.repouuid")?;
        assert!(harness.global_graph.find_branch(&format!("{}/master", uuid), BranchType::Local).is_ok());

        Ok(())
    })
}

/// The conflicts server should return the correct response when querying about a conflicting file.
#[test]
fn has_conflict_standard() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        change_and_commit(harness.local_repo_a, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./file_other.txt"), "new text a!")])?;

        let uuid_b = harness.local_repo_b.config()?.get_string("globalgraph.repouuid")?;
        let uuid_a = harness.local_repo_a.config()?.get_string("globalgraph.repouuid")?;

        let request = shared::ConflictsAfterCommitRequest {
            repo_uuid: uuid_b,
            files: vec![GitPath::new("filea.txt")],
            repo_head_commit: Some(CommitSha::new(&harness.local_repo_b.head()?.peel_to_commit()?.id().to_string())),
        };

        // When attempting to commit a change to filea.txt on repo b, there should be a conflict at the head of 
        // repo A.
        let response = make_conflicts_after_commit_request(harness.server, &request);
        let repo_a_head = CommitSha::new(&harness.local_repo_a.head()?.peel_to_commit()?.id().to_string());
        assert_eq!(
            response,
            serde_json::to_value(shared::ConflictsAfterCommitResponse {
                conflicts: vec!(shared::UnintegratedChange {
                    file: GitPath::new("filea.txt"),
                    commit: repo_a_head,
                    branch: ReferencePath("refs/heads/master".into()),
                    repo_uuid: uuid_a,
                })
            })?);

        return Ok(());
    })
}


/// Verifies conflicts are found by the pre-commit hook and the hook aborts properly.
#[test]
fn has_conflict_hooks_only() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        // Change filea.txt in local_repo_a.
        change_and_commit(harness.local_repo_a, &[(&PathBuf::from("./filea.txt"), "some text in a")])?;
        change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./file_other.txt"), "some other text")])?;

        // Modifying filea.txt in local_repo_b should fail, now.
        let result = change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./filea.txt"), "conflicting text in a")]);
        match result {
            Ok(_) => panic!("Local repo b should have returned an error when trying to commit."),
            Err(e) => assert!(String::from_utf8_lossy(&e.downcast::<CommandError>()?.output.stderr).contains("Exiting with status: [2]"))
        }

        return Ok(());
    })
}


/// Heads that are detached should still properly block commits if there is a conflict in a branch in the Global Graph
#[test]
fn detached_head_conflict() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        // Change filea.txt in local_repo_a.
        change_and_commit(harness.local_repo_a, &[(&PathBuf::from("./filea.txt"), "some text in a")])?;
        change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./file_other.txt"), "some other text")])?;

        git_cmd(harness.local_repo_b, &["checkout", "HEAD^"])?;
        assert!(harness.local_repo_b.head_detached()?);

        // Modifying filea.txt in local_repo_b should fail, now.
        let result = change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./filea.txt"), "conflicting text in a")]);
        match result {
            Ok(_) => panic!("Local repo b should have returned an error when trying to commit."),
            Err(e) => assert!(String::from_utf8_lossy(&e.downcast::<CommandError>()?.output.stderr).contains("Exiting with status: [2]"))
        }

        return Ok(());
    })
}

/// A simple integration test that makes sure committing different files produces no conflicts.
#[test]
fn no_conflicts() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        change_and_commit(harness.local_repo_a, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(harness.local_repo_b, &[(&PathBuf::from("./file_other.txt"), "new text a!")])?;

        // Verify that the server returns the correct result.
        let uuid = harness.local_repo_b.config()?.get_string("globalgraph.repouuid")?;
        let request = shared::ConflictsAfterCommitRequest {
            repo_uuid: uuid,
            files: vec![GitPath::new("./filea.txt")],
            repo_head_commit: Some(CommitSha::new(&harness.local_repo_b.head()?.peel_to_commit()?.id().to_string())),
        };
        let response = make_conflicts_after_commit_request(harness.server, &request);
        assert_eq!(
            response,
            serde_json::to_value(shared::ConflictsAfterCommitResponse {
                conflicts: vec!()
            })?);

        return Ok(());
    })
}


/// Tests sending the conflicts server a bad request json payload. It should return with the BAD_REQUEST error code.
#[test]
fn bad_data() -> Result<(), Error> {
    init_logging();

    create_integration_test(|harness| {
        // Bad data
        let request_json = object! {
        };
        let request = harness.server.client(http::Method::POST, "/v1/conflicts_after_commit")
            .content_type("application/json")
            .body(request_json.dump()).unwrap();

        let response = harness.server.execute(request.send()).unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        return Ok(());
    })
}

fn make_conflicts_after_commit_request(test_server: &mut TestServer, payload: &shared::ConflictsAfterCommitRequest) -> Value {
    let request = test_server.client(http::Method::POST, "/v1/conflicts_after_commit")
        .content_type("application/json")
        .body(serde_json::to_string(payload).unwrap()).unwrap();

    let response = test_server.execute(request.send()).unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = test_server.execute(response.body()).unwrap();
    let body = json::parse(str::from_utf8(&bytes).unwrap()).unwrap();
    return to_serde(body);
}


fn to_serde(value: JsonValue) -> Value {
    let value: Value = serde_json::from_str(&value.to_string()).unwrap();
    return value;
}