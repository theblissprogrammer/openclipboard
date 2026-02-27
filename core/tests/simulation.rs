//! Simulation tests using in-memory mocks.

use openclipboard_core::*;

#[tokio::test]
async fn two_peers_discover_each_other() {
    let disc = MockDiscovery::new_shared();
    let d2 = disc.clone_shared();

    disc.advertise(PeerInfo { peer_id: "a".into(), name: "Alice".into(), addr: "mem://a".into() }).await.unwrap();
    d2.advertise(PeerInfo { peer_id: "b".into(), name: "Bob".into(), addr: "mem://b".into() }).await.unwrap();

    let peers_a = disc.scan().await.unwrap();
    let peers_b = d2.scan().await.unwrap();

    assert_eq!(peers_a.len(), 2);
    assert_eq!(peers_b.len(), 2);
    assert!(peers_a.iter().any(|p| p.peer_id == "b"));
    assert!(peers_b.iter().any(|p| p.peer_id == "a"));
}

#[tokio::test]
async fn clipboard_text_sync() {
    let (conn_a, conn_b) = memory_connection_pair();
    let cb_a = MockClipboard::new();
    cb_a.write(ClipboardContent::Text("synced text".into())).unwrap();

    let session_a = Session::new(conn_a, MockIdentity::new("a"), cb_a);
    let session_b = Session::new(conn_b, MockIdentity::new("b"), MockClipboard::new());

    session_a.send_clipboard().await.unwrap();
    session_b.receive_clipboard().await.unwrap();

    assert_eq!(session_b.clipboard.read().unwrap(), ClipboardContent::Text("synced text".into()));
}

#[tokio::test]
async fn clipboard_image_sync() {
    let (conn_a, conn_b) = memory_connection_pair();
    let cb_a = MockClipboard::new();
    let img = ClipboardContent::Image {
        mime: "image/png".into(),
        width: 4,
        height: 4,
        bytes: vec![0x89, 0x50, 0x4E, 0x47, 1, 2, 3, 4],
    };
    cb_a.write(img.clone()).unwrap();

    let session_a = Session::new(conn_a, MockIdentity::new("a"), cb_a);
    let session_b = Session::new(conn_b, MockIdentity::new("b"), MockClipboard::new());

    session_a.send_clipboard().await.unwrap();
    session_b.receive_clipboard().await.unwrap();

    assert_eq!(session_b.clipboard.read().unwrap(), img);
}

#[tokio::test]
async fn file_offer_accept_transfer() {
    let (conn_a, conn_b) = memory_connection_pair();
    let session_a = Session::new(conn_a, MockIdentity::new("a"), MockClipboard::new());
    let session_b = Session::new(conn_b, MockIdentity::new("b"), MockClipboard::new());

    // A offers file
    session_a.send_file_offer("f1", "test.txt", 11, "text/plain").await.unwrap();
    let msg = session_b.recv_message().await.unwrap();
    match &msg {
        Message::FileOffer { file_id, name, size, .. } => {
            assert_eq!(file_id, "f1");
            assert_eq!(name, "test.txt");
            assert_eq!(*size, 11);
        }
        _ => panic!("expected FileOffer"),
    }

    // B accepts
    session_b.send_file_accept("f1").await.unwrap();
    let msg = session_a.recv_message().await.unwrap();
    assert!(matches!(msg, Message::FileAccept { file_id } if file_id == "f1"));

    // A sends chunk
    let data = b"hello world";
    session_a.send_file_chunk("f1", 0, data).await.unwrap();
    let msg = session_b.recv_message().await.unwrap();
    match msg {
        Message::FileChunk { file_id, offset, data_b64 } => {
            assert_eq!(file_id, "f1");
            assert_eq!(offset, 0);
            let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data_b64).unwrap();
            assert_eq!(decoded, data);
        }
        _ => panic!("expected FileChunk"),
    }

    // A sends done
    let hash = blake3::hash(data).to_hex().to_string();
    session_a.send_file_done("f1", &hash).await.unwrap();
    let msg = session_b.recv_message().await.unwrap();
    assert!(matches!(msg, Message::FileDone { file_id, hash: h } if file_id == "f1" && h == hash));
}

#[tokio::test]
async fn session_reconnect() {
    // First connection
    let (conn_a1, conn_b1) = memory_connection_pair();
    let cb_a = MockClipboard::new();
    cb_a.write(ClipboardContent::Text("before disconnect".into())).unwrap();
    let session_a1 = Session::new(conn_a1, MockIdentity::new("a"), MockClipboard::new());
    let session_b1 = Session::new(conn_b1, MockIdentity::new("b"), MockClipboard::new());

    session_a1.send_hello().await.unwrap();
    let msg = session_b1.recv_message().await.unwrap();
    assert!(matches!(msg, Message::Hello { .. }));

    // Disconnect
    session_a1.conn.close();
    assert!(session_a1.conn.is_closed());

    // Reconnect with new connections
    let (conn_a2, conn_b2) = memory_connection_pair();
    let cb_a2 = MockClipboard::new();
    cb_a2.write(ClipboardContent::Text("after reconnect".into())).unwrap();
    let session_a2 = Session::new(conn_a2, MockIdentity::new("a"), cb_a2);
    let session_b2 = Session::new(conn_b2, MockIdentity::new("b"), MockClipboard::new());

    session_a2.send_clipboard().await.unwrap();
    session_b2.receive_clipboard().await.unwrap();
    assert_eq!(session_b2.clipboard.read().unwrap(), ClipboardContent::Text("after reconnect".into()));
}
