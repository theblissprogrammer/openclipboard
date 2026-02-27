use openclipboard_core::{
    ClipboardContent, ClipboardProvider, Ed25519Identity, IdentityProvider, Listener,
    MemoryReplayProtector, MemoryTrustStore, Session, Transport, TrustRecord, TrustStore,
};
use openclipboard_core::clipboard::MockClipboard;
use openclipboard_core::quic_transport::{make_insecure_client_endpoint, make_server_endpoint, QuicListener, QuicTransport};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::test]
async fn e2e_quic_trusted_handshake_and_cliptext() {
    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();

    let trust_alice = Arc::new(MemoryTrustStore::new());
    trust_alice
        .save(TrustRecord {
            peer_id: bob.peer_id().to_string(),
            identity_pk: bob.public_key_bytes(),
            display_name: "Bob".into(),
            created_at: chrono::Utc::now(),
        })
        .unwrap();

    let trust_bob = Arc::new(MemoryTrustStore::new());
    trust_bob
        .save(TrustRecord {
            peer_id: alice.peer_id().to_string(),
            identity_pk: alice.public_key_bytes(),
            display_name: "Alice".into(),
            created_at: chrono::Utc::now(),
        })
        .unwrap();

    let replay_a = Arc::new(MemoryReplayProtector::new(128));
    let replay_b = Arc::new(MemoryReplayProtector::new(128));

    // Server (Bob)
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, _cert) = make_server_endpoint(bind).unwrap();
    let listener = QuicListener::new(endpoint);
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let server_task = {
        let trust_bob = trust_bob.clone();
        let replay_b = replay_b.clone();
        tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let session = Session::with_trust_and_replay(
                conn,
                bob,
                MockClipboard::new(),
                trust_bob,
                replay_b,
            );
            let peer = session.handshake().await.unwrap();
            tx.send(format!("handshake:{peer}")).unwrap();
            let msg = session.recv_message().await.unwrap();
            match msg {
                openclipboard_core::Message::ClipText { text, .. } => {
                    tx.send(format!("text:{text}")).unwrap();
                }
                other => {
                    tx.send(format!("unexpected:{:?}", other.msg_type())).unwrap();
                }
            }
        })
    };

    // Client (Alice)
    let endpoint = make_insecure_client_endpoint().unwrap();
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr.to_string()).await.unwrap();

    let cb = MockClipboard::new();
    cb.write(ClipboardContent::Text("hello over quic".into())).unwrap();

    let session = Session::with_trust_and_replay(conn, alice, cb, trust_alice, replay_a);
    session.handshake().await.unwrap();
    session.send_clipboard().await.unwrap();

    let got1 = rx.recv().await.unwrap();
    assert!(got1.starts_with("handshake:"));
    let got2 = rx.recv().await.unwrap();
    assert_eq!(got2, "text:hello over quic");

    server_task.await.unwrap();
}
