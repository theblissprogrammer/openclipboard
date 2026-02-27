//! QUIC loopback integration tests.

use openclipboard_core::protocol::{Frame, MsgType, StreamId, Message};
use openclipboard_core::transport::{Connection, Listener, Transport};
use openclipboard_core::quic_transport::{QuicListener, QuicTransport, make_server_endpoint, make_client_endpoint};

async fn setup() -> (QuicListener, QuicTransport, String) {
    let bind: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (endpoint, cert) = make_server_endpoint(bind).unwrap();
    let addr = endpoint.local_addr().unwrap();
    let listener = QuicListener::new(endpoint);
    let client_ep = make_client_endpoint(cert).unwrap();
    let transport = QuicTransport::new(client_ep);
    (listener, transport, addr.to_string())
}

fn msg_to_frame(msg: &Message, seq: u64) -> Frame {
    let payload = serde_json::to_vec(msg).unwrap();
    Frame::new(msg.msg_type(), msg.stream_id(), seq, payload)
}

#[tokio::test]
async fn quic_ping_pong() {
    let (listener, transport, addr) = setup().await;

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let frame = conn.recv().await.unwrap();
        assert_eq!(frame.msg_type, MsgType::Ping as u8);
        let pong = Frame::new(MsgType::Pong, StreamId::Control, 2, b"pong".to_vec());
        conn.send(pong).await.unwrap();
        // Wait for client to signal done
        let _ = conn.recv().await;
    });

    let conn = transport.connect(&addr).await.unwrap();
    let ping = Frame::new(MsgType::Ping, StreamId::Control, 1, b"ping".to_vec());
    conn.send(ping).await.unwrap();
    let resp = conn.recv().await.unwrap();
    assert_eq!(resp.msg_type, MsgType::Pong as u8);
    assert_eq!(resp.payload, b"pong");
    drop(conn);

    let _ = server.await;
}

#[tokio::test]
async fn quic_clipboard_text_sync() {
    let (listener, transport, addr) = setup().await;

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let frame = conn.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&frame.payload).unwrap();
        match msg {
            Message::ClipText { text, .. } => assert_eq!(text, "Hello from QUIC!"),
            _ => panic!("expected ClipText"),
        }
    });

    let conn = transport.connect(&addr).await.unwrap();
    let msg = Message::ClipText { mime: "text/plain".into(), text: "Hello from QUIC!".into(), ts_ms: 1 };
    conn.send(msg_to_frame(&msg, 1)).await.unwrap();

    server.await.unwrap();
}

#[tokio::test]
async fn quic_clipboard_image_sync() {
    let (listener, transport, addr) = setup().await;
    let fake_png = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    let expected = fake_png.clone();
    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        let frame = conn.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&frame.payload).unwrap();
        match msg {
            Message::ClipImage { bytes_b64, width, height, .. } => {
                assert_eq!(bytes_b64, expected);
                assert_eq!(width, 1);
                assert_eq!(height, 1);
            }
            _ => panic!("expected ClipImage"),
        }
    });

    let conn = transport.connect(&addr).await.unwrap();
    let msg = Message::ClipImage { mime: "image/png".into(), width: 1, height: 1, bytes_b64: fake_png, ts_ms: 2 };
    conn.send(msg_to_frame(&msg, 1)).await.unwrap();

    server.await.unwrap();
}

#[tokio::test]
async fn quic_file_transfer() {
    let (listener, transport, addr) = setup().await;
    let file_data = b"Hello file content over QUIC";
    let data_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, file_data);
    let hash = blake3::hash(file_data).to_hex().to_string();

    let expected_b64 = data_b64.clone();
    let expected_hash = hash.clone();
    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        // Receive FileOffer
        let f = conn.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&f.payload).unwrap();
        assert!(matches!(msg, Message::FileOffer { .. }));

        // Receive FileChunk
        let f = conn.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&f.payload).unwrap();
        match &msg {
            Message::FileChunk { data_b64, .. } => assert_eq!(data_b64, &expected_b64),
            _ => panic!("expected FileChunk"),
        }

        // Receive FileDone
        let f = conn.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&f.payload).unwrap();
        match msg {
            Message::FileDone { hash, .. } => assert_eq!(hash, expected_hash),
            _ => panic!("expected FileDone"),
        }
    });

    let conn = transport.connect(&addr).await.unwrap();
    let offer = Message::FileOffer { file_id: "f1".into(), name: "test.txt".into(), size: file_data.len() as u64, mime: "text/plain".into() };
    conn.send(msg_to_frame(&offer, 1)).await.unwrap();
    let chunk = Message::FileChunk { file_id: "f1".into(), offset: 0, data_b64 };
    conn.send(msg_to_frame(&chunk, 2)).await.unwrap();
    let done = Message::FileDone { file_id: "f1".into(), hash };
    conn.send(msg_to_frame(&done, 3)).await.unwrap();

    server.await.unwrap();
}

#[tokio::test]
async fn quic_bidirectional() {
    let (listener, transport, addr) = setup().await;

    let server = tokio::spawn(async move {
        let conn = listener.accept().await.unwrap();
        // Send clip from server side
        let msg = Message::ClipText { mime: "text/plain".into(), text: "from server".into(), ts_ms: 10 };
        conn.send(msg_to_frame(&msg, 1)).await.unwrap();
        // Receive clip from client
        let f = conn.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&f.payload).unwrap();
        match msg {
            Message::ClipText { text, .. } => assert_eq!(text, "from client"),
            _ => panic!("expected ClipText"),
        }
        // Keep connection alive until client reads
        let _ = conn.recv().await;
    });

    let conn = transport.connect(&addr).await.unwrap();
    // Send clip from client side
    let msg = Message::ClipText { mime: "text/plain".into(), text: "from client".into(), ts_ms: 20 };
    conn.send(msg_to_frame(&msg, 1)).await.unwrap();
    // Receive clip from server
    let f = conn.recv().await.unwrap();
    let got: Message = serde_json::from_slice(&f.payload).unwrap();
    match got {
        Message::ClipText { text, .. } => assert_eq!(text, "from server"),
        _ => panic!("expected ClipText"),
    }
    drop(conn);

    let _ = server.await;
}
