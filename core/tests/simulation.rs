use openclipboard_core::{
    derive_confirmation_code,
    pairing::PairingPayload,
    Ed25519Identity,
    IdentityProvider,
    MemoryTrustStore,
    MockClipboard,
    Session,
    TrustRecord,
    TrustStore,
};

use openclipboard_core::transport::memory_connection_pair;
use chrono::Utc;

#[tokio::test]
async fn full_pairing_and_trusted_handshake_flow() {
    // Two peers with real Ed25519 identities
    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();

    // Pairing exchange (QR-like)
    let nonce = vec![7u8; 32];
    let alice_payload = PairingPayload {
        version: 1,
        peer_id: alice.peer_id().to_string(),
        name: "Alice".into(),
        identity_pk: alice.public_key_bytes(),
        lan_port: 18455,
        nonce: nonce.clone(),
        lan_addrs: vec![],
    };

    let bob_payload = PairingPayload {
        version: 1,
        peer_id: bob.peer_id().to_string(),
        name: "Bob".into(),
        identity_pk: bob.public_key_bytes(),
        lan_port: 18456,
        nonce: vec![],
        lan_addrs: vec![],
    };

    let alice_code = derive_confirmation_code(&alice_payload.nonce, &alice_payload.peer_id, &bob_payload.peer_id);
    let bob_code = derive_confirmation_code(&alice_payload.nonce, &alice_payload.peer_id, &bob_payload.peer_id);
    assert_eq!(alice_code, bob_code);
    assert_eq!(alice_code.len(), 6);

    // Trust stores updated after user confirms code
    let alice_trust = std::sync::Arc::new(MemoryTrustStore::new());
    alice_trust
        .save(TrustRecord {
            peer_id: bob_payload.peer_id.clone(),
            identity_pk: bob_payload.identity_pk.clone(),
            display_name: bob_payload.name.clone(),
            created_at: Utc::now(),
        })
        .unwrap();

    let bob_trust = std::sync::Arc::new(MemoryTrustStore::new());
    bob_trust
        .save(TrustRecord {
            peer_id: alice_payload.peer_id.clone(),
            identity_pk: alice_payload.identity_pk.clone(),
            display_name: alice_payload.name.clone(),
            created_at: Utc::now(),
        })
        .unwrap();

    // Now they can connect and handshake with trust verification
    let (conn_a, conn_b) = memory_connection_pair();

    let session_a = Session::with_trust(conn_a, alice, MockClipboard::new(), alice_trust);
    let session_b = Session::with_trust(conn_b, bob, MockClipboard::new(), bob_trust);

    let (ra, rb) = tokio::join!(session_a.handshake(), session_b.handshake());
    assert_eq!(ra.unwrap(), session_b.identity.peer_id());
    assert_eq!(rb.unwrap(), session_a.identity.peer_id());
}
