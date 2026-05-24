// SPDX-License-Identifier: Apache-2.0
//! Wire-level integration tests against a mocked Integritas v2 API.
//!
//! These pin the request body shape (`api_key` + `file_hash` + optional
//! `filename`/`filesize`) and the response parsing path. If Integritas
//! ever reshapes the v2 schema, this is the one place that has to update.

use integritas_edge::{IntegritasClient, IntegritasConfig};
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn hex_hash(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn client_for(server: &MockServer) -> IntegritasClient {
    IntegritasClient::new(IntegritasConfig {
        api_key: "ik_test_abc".into(),
        base_url: Some(server.uri()),
        timeout: None,
    })
    .expect("client builds")
}

#[tokio::test]
async fn stamp_posts_expected_body_and_parses_success() {
    let server = MockServer::start().await;
    let expected_body = json!({
        "api_key": "ik_test_abc",
        "file_hash": hex_hash(0xAB),
        "filename": "evidence.bin",
        "filesize": 42,
    });

    Mock::given(method("POST"))
        .and(path("/v2/timestamp/post"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "success",
            "data": { "txid": "0xDEADBEEF", "nft_id": "nft_42" },
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let resp = client
        .stamp(
            &[0xAB; 32],
            Some(integritas_edge::StampMetadata {
                filename: Some("evidence.bin".into()),
                filesize: Some(42),
            }),
        )
        .await
        .expect("stamp ok");
    assert_eq!(resp.status, "success");
    assert_eq!(resp.data["txid"], "0xDEADBEEF");
    assert!(resp.error.is_none());
}

#[tokio::test]
async fn stamp_without_metadata_omits_filename_filesize() {
    let server = MockServer::start().await;
    // Metadata fields must be *absent*, not null — the request struct
    // uses `skip_serializing_if = "Option::is_none"`.
    let expected_body = json!({
        "api_key": "ik_test_abc",
        "file_hash": hex_hash(0x11),
    });

    Mock::given(method("POST"))
        .and(path("/v2/timestamp/post"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "success",
            "data": {},
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    client
        .stamp(&[0x11; 32], None)
        .await
        .expect("stamp ok with no metadata");
}

#[tokio::test]
async fn verify_hits_verify_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/verify/file"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "success",
            "links": { "report": "https://integritas.technology/report/abc.pdf" },
            "data": { "block_height": 142_057 },
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let resp = client.verify(&[0xCD; 32], None).await.expect("verify ok");
    assert_eq!(resp.status, "success");
    assert_eq!(resp.data["block_height"], 142_057);
}

#[tokio::test]
async fn check_hits_file_check_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/file/check"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "success",
            "data": { "exists": true },
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let resp = client.check(&[0xEF; 32]).await.expect("check ok");
    assert_eq!(resp.status, "success");
    assert_eq!(resp.data["exists"], true);
}

#[tokio::test]
async fn non_2xx_surfaces_http_status_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/timestamp/post"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.stamp(&[0x00; 32], None).await.unwrap_err();
    match err {
        integritas_edge::Error::HttpStatus { status, body } => {
            assert_eq!(status, 401);
            assert!(body.contains("unauthorized"));
        }
        other => panic!("expected HttpStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn malformed_response_surfaces_malformed_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/timestamp/post"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json at all"))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.stamp(&[0x00; 32], None).await.unwrap_err();
    assert!(matches!(
        err,
        integritas_edge::Error::Malformed { .. }
    ));
}

#[tokio::test]
async fn empty_body_on_200_is_malformed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/timestamp/post"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.stamp(&[0x00; 32], None).await.unwrap_err();
    assert!(matches!(
        err,
        integritas_edge::Error::Malformed { .. }
    ));
}
