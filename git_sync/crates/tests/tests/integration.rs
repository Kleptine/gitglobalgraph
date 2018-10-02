extern crate test_utilities;
extern crate actix_web;
extern crate serde_json;
extern crate server;

#[macro_use]
extern crate json;

use test_utilities::*;
use std::error::Error;
use std::path::PathBuf;
use actix_web::http;
use http::StatusCode;
use actix_web::{HttpRequest, HttpMessage};
use actix_web::test::TestServer;
use std::str;
use json::JsonValue;
use serde_json::value::Value;

#[test]
fn single_push() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_integration_test(|local_repo, global_repo, test_server| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "new text b!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 1);
        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        // Verify that the server returns the correct result.
        let request_json = object! {
            "head" => "test",
            "files" => array!["file1.bin", "file2.bin"]
        };
        let response = make_conflicts_after_commit_request(test_server, &request_json);
        assert_eq!(
            response,
            serde_json::to_value(server::ConflictsAfterCommitResponse {
                conflicts: vec!()
            })?);

        return Ok(());
    })
}

#[test]
fn conflict() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_integration_test(|local_repo, global_repo, test_server| {
        change_and_commit(local_repo, &[(&PathBuf::from("./filea.txt"), "new text a!")])?;
        change_and_commit(local_repo, &[(&PathBuf::from("./fileb.txt"), "new text b!")])?;

        // Verify that the post-commit hooks properly pushed to the server.
        assert_eq!(global_repo.branches(None)?.count(), 1);
        assert_eq!(local_repo.all_commits(), global_repo.all_commits());

        // Verify that the server returns the correct result.
        let request_json = object! {
            "head" => "test",
            "files" => array!["file1.bin", "file2.bin"]
        };
        let response = make_conflicts_after_commit_request(test_server, &request_json);
        assert_eq!(
            response,
            serde_json::to_value(server::ConflictsAfterCommitResponse {
                conflicts: vec!()
            })?);

        return Ok(());
    })
}


#[test]
fn bad_data() -> Result<(), Box<Error>> {
    init_logging().unwrap();

    create_integration_test(|local_repo, global_repo, test_server| {
        // Bad data
        let request_json = object! {
        };
        let request = test_server.client(http::Method::POST, "/v1/conflicts_after_commit")
            .content_type("application/json")
            .body(request_json.dump()).unwrap();

        let response = test_server.execute(request.send()).unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        return Ok(());
    })
}

fn make_conflicts_after_commit_request(test_server: &mut TestServer, request: &JsonValue) -> Value {
    let request = test_server.client(http::Method::POST, "/v1/conflicts_after_commit")
        .content_type("application/json")
        .body(request.dump()).unwrap();

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