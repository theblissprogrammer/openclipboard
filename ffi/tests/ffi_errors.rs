use base64::Engine as _;

use openclipboard_ffi::{
    identity_generate,
    identity_load,
    pairing_payload_from_qr_string,
    trust_store_default_path,
    trust_store_open,
};

#[test]
fn identity_load_missing_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.json");
    assert!(identity_load(path.to_string_lossy().to_string()).is_err());
}

#[test]
fn identity_save_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a/b/c/identity.json");

    let id = identity_generate();
    id.save(nested.to_string_lossy().to_string()).unwrap();

    let loaded = identity_load(nested.to_string_lossy().to_string()).unwrap();
    assert_eq!(id.peer_id(), loaded.peer_id());
}

#[test]
fn pairing_payload_from_qr_string_invalid_errors() {
    // Not base64url, should fail
    assert!(pairing_payload_from_qr_string("not-a-qr".into()).is_err());
}

#[test]
fn trust_store_open_and_add_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("x/y/z/trust.json");

    let store = trust_store_open(nested.to_string_lossy().to_string()).unwrap();

    // invalid base64 should error
    assert!(store
        .add("peer-x".into(), "$$$".into(), "X".into())
        .is_err());

    // valid base64 works and should create dirs + persist
    let pk_b64 = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4]);
    store
        .add("peer-x".into(), pk_b64.clone(), "Xavier".into())
        .unwrap();

    let got = store.get("peer-x".into()).unwrap().unwrap();
    assert_eq!(got.peer_id, "peer-x");
    assert_eq!(got.identity_pk_b64, pk_b64);
}

#[test]
fn trust_store_default_path_returns_something() {
    let p = trust_store_default_path();
    assert!(!p.is_empty());
    assert!(p.contains("trust"));
}
