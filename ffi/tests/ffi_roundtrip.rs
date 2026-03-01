use base64::Engine as _;

use openclipboard_ffi::{
    default_identity_path,
    derive_confirmation_code,
    identity_generate,
    identity_load,
    pairing_payload_create,
    pairing_payload_from_qr_string,
    trust_store_open,
};

#[test]
fn identity_generate_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.json");

    let id1 = identity_generate();
    let peer1 = id1.peer_id();

    id1.save(path.to_string_lossy().to_string()).unwrap();

    let id2 = identity_load(path.to_string_lossy().to_string()).unwrap();
    assert_eq!(peer1, id2.peer_id());
    assert_eq!(id1.pubkey_b64(), id2.pubkey_b64());
}

#[test]
fn pairing_payload_qr_roundtrip() {
    let id = identity_generate();

    let payload = pairing_payload_create(
        1,
        id.peer_id(),
        "Alice".to_string(),
        base64::engine::general_purpose::STANDARD
            .decode(id.pubkey_b64())
            .unwrap(),
        18455,
        vec![7u8; 32],
        vec!["192.168.1.10".to_string()],
    );

    let s = payload.to_qr_string().unwrap();
    let decoded = pairing_payload_from_qr_string(s).unwrap();

    assert_eq!(payload.version(), decoded.version());
    assert_eq!(payload.peer_id(), decoded.peer_id());
    assert_eq!(payload.name(), decoded.name());
    assert_eq!(payload.identity_pk(), decoded.identity_pk());
    assert_eq!(payload.lan_port(), decoded.lan_port());
    assert_eq!(payload.nonce(), decoded.nonce());
}

#[test]
fn derive_confirmation_code_deterministic() {
    let nonce = vec![42u8; 32];
    let c1 = derive_confirmation_code(nonce.clone(), "peer-a".into(), "peer-b".into());
    let c2 = derive_confirmation_code(nonce, "peer-a".into(), "peer-b".into());
    assert_eq!(c1, c2);
    assert_eq!(c1.len(), 6);
}

#[test]
fn trust_store_add_list_get_remove_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("trust.json");

    let store = trust_store_open(path.to_string_lossy().to_string()).unwrap();

    let pk_b64 = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4]);

    store
        .add("peer-x".into(), pk_b64.clone(), "Xavier".into())
        .unwrap();

    let got = store.get("peer-x".into()).unwrap().unwrap();
    assert_eq!(got.peer_id, "peer-x");
    assert_eq!(got.identity_pk_b64, pk_b64);
    assert_eq!(got.display_name, "Xavier");
    assert!(got.created_at_ms > 0);

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);

    assert!(store.remove("peer-x".into()).unwrap());
    assert!(!store.remove("peer-x".into()).unwrap());

    let list2 = store.list().unwrap();
    assert_eq!(list2.len(), 0);
}

#[test]
fn default_identity_path_returns_something() {
    let p = default_identity_path();
    assert!(!p.is_empty());
    assert!(p.contains("identity"));
}
