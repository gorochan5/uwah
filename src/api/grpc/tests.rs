use super::*;
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::metadata::MetadataValue;
use tonic::Request;

const PLAINTEXT_SECRET: &str = "super-secret-token-12345";

fn create_test_request(auth_header: Option<&str>) -> Request<()> {
    let mut req = Request::new(());
    if let Some(header_val) = auth_header
        && let Ok(meta_val) = MetadataValue::try_from(header_val)
    {
        req.metadata_mut().insert("authorization", meta_val);
    }
    req
}

#[test]
fn test_valid_plaintext_token() {
    let interceptor = grpc_auth_interceptor(PLAINTEXT_SECRET.to_string());
    let req = create_test_request(Some(&format!("Bearer {}", PLAINTEXT_SECRET)));
    assert!(interceptor(req).is_ok());
}

#[test]
fn test_valid_sha256_digest_token() {
    let sha256_secret = hex::encode(Sha256::digest(PLAINTEXT_SECRET.as_bytes()));
    let interceptor = grpc_auth_interceptor(sha256_secret);
    let req = create_test_request(Some(&format!("Bearer {}", PLAINTEXT_SECRET)));
    assert!(interceptor(req).is_ok());
}

#[test]
fn test_missing_metadata() {
    let interceptor = grpc_auth_interceptor(PLAINTEXT_SECRET.to_string());
    let req = create_test_request(None);
    let res = interceptor(req);
    assert!(res.is_err());
    let status = res.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert_eq!(status.message(), "unauthenticated");
}

#[test]
fn test_invalid_token() {
    let interceptor = grpc_auth_interceptor(PLAINTEXT_SECRET.to_string());
    let req = create_test_request(Some("Bearer wrong-secret-token"));
    let res = interceptor(req);
    assert!(res.is_err());
    let status = res.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert_eq!(status.message(), "unauthenticated");
}

#[test]
fn test_header_without_bearer_prefix() {
    let interceptor = grpc_auth_interceptor(PLAINTEXT_SECRET.to_string());
    let req = create_test_request(Some(PLAINTEXT_SECRET));
    let res = interceptor(req);
    assert!(res.is_err());
    let status = res.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert_eq!(status.message(), "unauthenticated");
}

#[test]
fn test_empty_token() {
    let interceptor = grpc_auth_interceptor(PLAINTEXT_SECRET.to_string());
    let req = create_test_request(Some("Bearer "));
    let res = interceptor(req);
    assert!(res.is_err());
    let status = res.err().unwrap();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert_eq!(status.message(), "unauthenticated");
}

#[test]
fn test_file_descriptor_set_integrity_and_reflection() {
    assert!(
        !proto::FILE_DESCRIPTOR_SET.is_empty(),
        "Encoded FileDescriptorSet must not be empty"
    );

    let descriptor_set =
        prost_types::FileDescriptorSet::decode(proto::FILE_DESCRIPTOR_SET)
            .expect("Should decode proto file descriptor set");

    let service_names: Vec<String> = descriptor_set
        .file
        .iter()
        .flat_map(|f| {
            let pkg = f.package();
            f.service.iter().map(move |s| format!("{}.{}", pkg, s.name()))
        })
        .collect();

    let expected_services = [
        "uwah.v1.HealthService",
        "uwah.v1.EventStreamService",
        "uwah.v1.SessionService",
        "uwah.v1.ChatService",
        "uwah.v1.GroupService",
        "uwah.v1.GroupParticipantService",
        "uwah.v1.MessageService",
        "uwah.v1.ContactService",
        "uwah.v1.CallService",
        "uwah.v1.MediaService",
        "uwah.v1.WebhookDlqService",
    ];

    for expected in &expected_services {
        assert!(
            service_names.iter().any(|s| s == expected),
            "Service {} must be registered in the reflection file descriptor set",
            expected
        );
    }
}

#[test]
fn test_grpc_status_mappings_and_errors() {
    let unauthenticated = tonic::Status::unauthenticated("unauthenticated");
    assert_eq!(unauthenticated.code(), tonic::Code::Unauthenticated);

    let not_found = tonic::Status::not_found("Resource not found");
    assert_eq!(not_found.code(), tonic::Code::NotFound);

    let invalid_argument = tonic::Status::invalid_argument("Invalid SHA-256");
    assert_eq!(invalid_argument.code(), tonic::Code::InvalidArgument);

    let permission_denied = tonic::Status::permission_denied("Access denied");
    assert_eq!(permission_denied.code(), tonic::Code::PermissionDenied);

    let out_of_range = tonic::Status::out_of_range("Offset exceeds length");
    assert_eq!(out_of_range.code(), tonic::Code::OutOfRange);

    let internal = tonic::Status::internal("Database error");
    assert_eq!(internal.code(), tonic::Code::Internal);
}
