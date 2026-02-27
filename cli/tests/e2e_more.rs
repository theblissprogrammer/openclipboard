use base64::Engine as _;
use openclipboard::{pairing_finalize, pairing_init_qr, pairing_respond_qr};
use openclipboard_core::clipboard::MockClipboard;
use openclipboard_core::identity::IdentityProvider;
use openclipboard_core::quic_transport::{
    make_insecure_client_endpoint, make_server_endpoint, QuicListener, QuicTransport,
};
use openclipboard_core::{
    ClipboardContent, ClipboardProvider, Ed25519Identity, Listener, MemoryReplayProtector,
    MemoryTrustStore, Session, Transport, TrustRecord, TrustStore,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

fn trust_from_pairing(alice: &Ed25519Identity, bob: &Ed25519Identity) -> (Arc<MemoryTrustStore>, Arc<MemoryTrustStore>) {
    let init_qr = pairing_init_qr("Alice".into(), 1234, alice, [1u8; 32]);
    let (resp_qr, _code) = pairing_respond_qr(&init_qr, "Bob".into(), 2345, bob).unwrap();
    let (_code2, recs) = pairing_finalize(&init_qr, &resp_qr).unwrap();

    let trust_a = Arc::new(MemoryTrustStore::new());
    let trust_b = Arc::new(MemoryTrustStore::new());

    // Each side trusts the other.
    let rec_alice = recs[0].peer_id.clone();
    let rec_bob = recs[1].peer_id.clone();

    for rec in recs {
        if rec.peer_id == rec_alice {
            trust_b
                .save(TrustRecord {
                    peer_id: rec.peer_id,
                    identity_pk: rec.identity_pk,
                    display_name: rec.display_name,
                    created_at: chrono::Utc::now(),
                })
                .unwrap();
        } else if rec.peer_id == rec_bob {
            trust_a
                .save(TrustRecord {
                    peer_id: rec.peer_id,
                    identity_pk: rec.identity_pk,
                    display_name: rec.display_name,
                    created_at: chrono::Utc::now(),
                })
                .unwrap();
        }
    }

    (trust_a, trust_b)
}

async fn recv_with_timeout<C, I, CB>(
    session: &Session<C, I, CB>,
    dur: Duration,
) -> openclipboard_core::Message
where
    C: openclipboard_core::Connection,
    I: openclipboard_core::IdentityProvider,
    CB: openclipboard_core::ClipboardProvider,
{
    tokio::time::timeout(dur, session.recv_message())
        .await
        .expect("timeout")
        .expect("recv")
}

#[tokio::test]
async fn e2e_pair_and_send_text() {
    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();
    let (trust_a, trust_b) = trust_from_pairing(&alice, &bob);

    let replay_a = Arc::new(MemoryReplayProtector::new(128));
    let replay_b = Arc::new(MemoryReplayProtector::new(128));

    // Server (Bob)
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, _cert) = make_server_endpoint(bind).unwrap();
    let listener = QuicListener::new(endpoint);
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let session = Session::with_trust_and_replay(conn, bob, MockClipboard::new(), trust_b, replay_b);
        let peer = session.handshake().await.unwrap();
        tx.send(format!("handshake:{peer}")).unwrap();
        let msg = recv_with_timeout(&session, Duration::from_secs(2)).await;
        match msg {
            openclipboard_core::Message::ClipText { text, .. } => tx.send(format!("text:{text}")).unwrap(),
            other => tx.send(format!("unexpected:{:?}", other.msg_type())).unwrap(),
        }
    });

    // Client (Alice)
    let endpoint = make_insecure_client_endpoint().unwrap();
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr.to_string()).await.unwrap();

    let cb = MockClipboard::new();
    cb.write(ClipboardContent::Text("paired hello".into())).unwrap();

    let session = Session::with_trust_and_replay(conn, alice, cb, trust_a, replay_a);
    session.handshake().await.unwrap();
    session.send_clipboard().await.unwrap();

    assert!(rx.recv().await.unwrap().starts_with("handshake:"));
    assert_eq!(rx.recv().await.unwrap(), "text:paired hello");

    server.await.unwrap();
}

#[tokio::test]
async fn e2e_pair_and_send_image() {
    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();
    let (trust_a, trust_b) = trust_from_pairing(&alice, &bob);

    let replay_a = Arc::new(MemoryReplayProtector::new(128));
    let replay_b = Arc::new(MemoryReplayProtector::new(128));

    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, _cert) = make_server_endpoint(bind).unwrap();
    let listener = QuicListener::new(endpoint);
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let session = Session::with_trust_and_replay(conn, bob, MockClipboard::new(), trust_b, replay_b);
        session.handshake().await.unwrap();
        let msg = recv_with_timeout(&session, Duration::from_secs(2)).await;
        match msg {
            openclipboard_core::Message::ClipImage { mime, width, height, bytes_b64, .. } => {
                tx.send(format!("img:{mime}:{width}x{height}:{}", bytes_b64.len())).unwrap();
            }
            other => tx.send(format!("unexpected:{:?}", other.msg_type())).unwrap(),
        }
    });

    let endpoint = make_insecure_client_endpoint().unwrap();
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr.to_string()).await.unwrap();

    // 1x1 PNG (very small) is fine; bytes don't need to be a valid PNG for transport.
    let png_bytes = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0];
    let cb = MockClipboard::new();
    cb.write(ClipboardContent::Image {
        mime: "image/png".into(),
        width: 1,
        height: 1,
        bytes: png_bytes,
    })
    .unwrap();

    let session = Session::with_trust_and_replay(conn, alice, cb, trust_a, replay_a);
    session.handshake().await.unwrap();
    session.send_clipboard().await.unwrap();

    let got = rx.recv().await.unwrap();
    assert!(got.starts_with("img:image/png:1x1:"));

    server.await.unwrap();
}

async fn e2e_send_file_case(size: usize) {
    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();
    let (trust_a, trust_b) = trust_from_pairing(&alice, &bob);

    let replay_a = Arc::new(MemoryReplayProtector::new(128));
    let replay_b = Arc::new(MemoryReplayProtector::new(128));

    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, _cert) = make_server_endpoint(bind).unwrap();
    let listener = QuicListener::new(endpoint);
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Deterministic bytes.
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i as u8).wrapping_mul(31).wrapping_add(7));
    }
    let expected_hash = blake3::hash(&data).to_hex().to_string();

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let session = Session::with_trust_and_replay(conn, bob, MockClipboard::new(), trust_b, replay_b);
        session.handshake().await.unwrap();

        let mut buf: Vec<u8> = Vec::new();
        let mut want_file_id: Option<String> = None;
        let mut want_size: Option<u64> = None;

        loop {
            let msg = recv_with_timeout(&session, Duration::from_secs(5)).await;
            match msg {
                openclipboard_core::Message::FileOffer { file_id, size, .. } => {
                    want_file_id = Some(file_id.clone());
                    want_size = Some(size);
                    session.send_file_accept(&file_id).await.unwrap();
                }
                openclipboard_core::Message::FileChunk { file_id, data_b64, .. } => {
                    if want_file_id.as_deref() == Some(&file_id) {
                        let bytes = base64::engine::general_purpose::STANDARD.decode(data_b64).unwrap();
                        buf.extend_from_slice(&bytes);
                    }
                }
                openclipboard_core::Message::FileDone { file_id, hash } => {
                    if want_file_id.as_deref() == Some(&file_id) {
                        tx.send(format!("done:{}:{}", buf.len(), hash)).unwrap();
                        if let Some(sz) = want_size {
                            assert_eq!(buf.len() as u64, sz);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }

        // Keep task alive only until done.
    });

    let endpoint = make_insecure_client_endpoint().unwrap();
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr.to_string()).await.unwrap();

    let session = Session::with_trust_and_replay(conn, alice, MockClipboard::new(), trust_a, replay_a);
    session.handshake().await.unwrap();

    let file_id = blake3::hash(format!("file.bin:{}", data.len()).as_bytes())
        .to_hex()
        .to_string();
    session
        .send_file_offer(&file_id, "file.bin", data.len() as u64, "application/octet-stream")
        .await
        .unwrap();

    // Wait briefly for accept to reduce flakiness.
    let _ = tokio::time::timeout(Duration::from_secs(1), session.recv_message()).await;

    const CHUNK: usize = 64 * 1024;
    let mut offset = 0u64;
    for chunk in data.chunks(CHUNK) {
        session.send_file_chunk(&file_id, offset, chunk).await.unwrap();
        offset += chunk.len() as u64;
    }

    session.send_file_done(&file_id, &expected_hash).await.unwrap();

    let got = rx.recv().await.unwrap();
    let parts: Vec<&str> = got.split(':').collect();
    assert_eq!(parts[0], "done");
    assert_eq!(parts[1].parse::<usize>().unwrap(), data.len());
    assert_eq!(parts[2], expected_hash);

    server.await.unwrap();
}

#[tokio::test]
async fn e2e_pair_and_send_file_small() {
    e2e_send_file_case(32 * 1024).await;
}

#[tokio::test]
async fn e2e_pair_and_send_file_large() {
    // ~8 MiB, big enough to exercise chunking but still fast.
    e2e_send_file_case(8 * 1024 * 1024).await;
}

#[tokio::test]
async fn e2e_reject_untrusted() {
    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();

    // No trust on either side.
    let trust_a = Arc::new(MemoryTrustStore::new());
    let trust_b = Arc::new(MemoryTrustStore::new());
    let replay_a = Arc::new(MemoryReplayProtector::new(128));
    let replay_b = Arc::new(MemoryReplayProtector::new(128));

    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, _cert) = make_server_endpoint(bind).unwrap();
    let listener = QuicListener::new(endpoint);
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let session = Session::with_trust_and_replay(conn, bob, MockClipboard::new(), trust_b, replay_b);
        let res = session.handshake_with_timeout(Duration::from_secs(2)).await;
        tx.send(format!("server_ok:{}", res.is_ok())).unwrap();
    });

    let endpoint = make_insecure_client_endpoint().unwrap();
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr.to_string()).await.unwrap();
    let session = Session::with_trust_and_replay(conn, alice, MockClipboard::new(), trust_a, replay_a);
    let res = session.handshake_with_timeout(Duration::from_secs(2)).await;
    assert!(res.is_err());

    let got = rx.recv().await.unwrap();
    assert_eq!(got, "server_ok:false");

    server.await.unwrap();
}

#[derive(Clone)]
struct SpoofIdentity {
    claimed_peer_id: String,
    signing: ed25519_dalek::SigningKey,
}

impl IdentityProvider for SpoofIdentity {
    fn peer_id(&self) -> &str {
        &self.claimed_peer_id
    }

    fn sign(&self, data: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer as _;
        self.signing.sign(data).to_bytes().to_vec()
    }

    fn verify(&self, _peer_id: &str, _data: &[u8], _signature: &[u8]) -> bool {
        // Not used by the handshake verifier (it uses Ed25519Identity::verify_with_public_key).
        false
    }

    fn public_key_bytes(&self) -> Vec<u8> {
        self.signing.verifying_key().as_bytes().to_vec()
    }
}

#[tokio::test]
async fn e2e_reject_spoofed_peer() {
    let victim = Ed25519Identity::generate();
    let server_id = Ed25519Identity::generate();

    // Server trusts victim.
    let trust_server = Arc::new(MemoryTrustStore::new());
    trust_server
        .save(TrustRecord {
            peer_id: victim.peer_id().to_string(),
            identity_pk: victim.public_key_bytes(),
            display_name: "Victim".into(),
            created_at: chrono::Utc::now(),
        })
        .unwrap();

    // Client will claim victim's peer_id but present attacker's pk.
    let attacker = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
    let spoof = SpoofIdentity {
        claimed_peer_id: victim.peer_id().to_string(),
        signing: attacker,
    };

    let replay = Arc::new(MemoryReplayProtector::new(128));

    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, _cert) = make_server_endpoint(bind).unwrap();
    let listener = QuicListener::new(endpoint);
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let session = Session::with_trust_and_replay(
            conn,
            server_id,
            MockClipboard::new(),
            trust_server,
            replay,
        );
        let res = session.handshake_with_timeout(Duration::from_secs(2)).await;
        tx.send(format!("server_ok:{}", res.is_ok())).unwrap();
    });

    let endpoint = make_insecure_client_endpoint().unwrap();
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr.to_string()).await.unwrap();

    let trust_client = Arc::new(MemoryTrustStore::new());
    let replay_client = Arc::new(MemoryReplayProtector::new(128));
    let session = Session::with_pairing_mode_and_replay(conn, spoof, MockClipboard::new(), trust_client, replay_client);
    // Client-side handshake will also fail because the server won't accept.
    assert!(session.handshake_with_timeout(Duration::from_secs(2)).await.is_err());

    assert_eq!(rx.recv().await.unwrap(), "server_ok:false");

    server.await.unwrap();
}
