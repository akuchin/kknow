//! What a person sees when S3 refuses: a fake S3 server gives the same
//! answers as AWS and MinIO, so these tests need no network and no keys.

#![cfg(feature = "s3")]

use kknow_sync::provider::Cause;
use kknow_sync::{S3Config, S3Provider, SyncProvider};

/// A local server that answers each request with the same S3 error.
fn fake_s3(status: u16, code: &str, message: &str) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("a free port");
    let address = format!(
        "http://{}",
        server.server_addr().to_ip().expect("an IP address")
    );
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>{code}</Code><Message>{message}</Message></Error>"#
    );
    std::thread::spawn(move || {
        let header: tiny_http::Header = "Content-Type: application/xml".parse().expect("a header");
        for request in server.incoming_requests() {
            let response = tiny_http::Response::from_string(body.clone())
                .with_status_code(status)
                .with_header(header.clone());
            let _ = request.respond(response);
        }
    });
    address
}

fn s3(endpoint: String) -> S3Provider {
    S3Provider::connect(&S3Config {
        bucket: "notes".into(),
        endpoint: Some(endpoint),
        access_key_id: Some("wrong".into()),
        secret_access_key: Some("wrong".into()),
        ..S3Config::default()
    })
    .expect("connect does not use the network")
}

#[test]
fn a_wrong_key_is_named_with_the_code_of_the_storage() {
    let storage = s3(fake_s3(
        403,
        "InvalidAccessKeyId",
        "The Access Key Id you provided does not exist in our records.",
    ));

    for error in [
        storage.list("").unwrap_err(),
        storage.get("a.md").unwrap_err(),
    ] {
        assert_eq!(error.cause, Cause::KeysRefused("InvalidAccessKeyId".into()));
        let text = error.to_string();
        assert!(text.contains("access keys"), "{text}");
        assert!(!text.contains("context"), "the library text leaks: {text}");
    }
}

#[test]
fn a_wrong_secret_is_a_refused_key_too() {
    let storage = s3(fake_s3(
        403,
        "SignatureDoesNotMatch",
        "The request signature we calculated does not match.",
    ));
    assert_eq!(
        storage.list("").unwrap_err().cause,
        Cause::KeysRefused("SignatureDoesNotMatch".into())
    );
}

#[test]
fn a_missing_bucket_is_named() {
    let storage = s3(fake_s3(
        404,
        "NoSuchBucket",
        "The specified bucket does not exist",
    ));

    let errors = [
        storage.list("").unwrap_err(),
        storage.get("a.md").unwrap_err(),
        storage.put("a.md", b"# A").unwrap_err(),
    ];
    for error in errors {
        assert_eq!(error.cause, Cause::NoBucket("notes".into()));
        assert!(
            error.to_string().contains("bucket notes does not exist"),
            "{error}"
        );
    }
}

#[test]
fn no_answer_names_the_address() {
    // Port 1 on the local machine: nothing listens there, and the refusal is
    // immediate, with no network.
    let error = s3("http://127.0.0.1:1".into()).list("").unwrap_err();
    assert_eq!(error.cause, Cause::Unreachable("http://127.0.0.1:1".into()));
    assert!(error.to_string().contains("does not answer"), "{error}");
}

#[test]
fn another_error_keeps_a_short_text_of_the_library() {
    let storage = s3(fake_s3(
        500,
        "InternalError",
        "We encountered an internal error.",
    ));
    let error = storage.put("a.md", b"# A").unwrap_err();
    assert_eq!(error.cause, Cause::Other);
    let text = error.to_string();
    assert!(text.contains("InternalError"), "{text}");
    assert!(!text.contains("headers"), "the HTTP headers leak: {text}");
}
