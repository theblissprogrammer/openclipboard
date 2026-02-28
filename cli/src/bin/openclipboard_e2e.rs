//! CI-oriented end-to-end harness for openclipboard.
//!
//! This is intentionally simple and runner-friendly:
//! - Runs on a single machine (GitHub Actions runner)
//! - Uses real QUIC transport + Session handshake + message framing
//! - Avoids BLE, OS clipboard APIs, or UI

use anyhow::Context;
use clap::{Parser, Subcommand};
use openclipboard_core::{
    quic_transport::{make_insecure_client_endpoint, make_server_endpoint, QuicListener, QuicTransport},
    ClipboardContent, ClipboardProvider, Ed25519Identity, Message, MockClipboard, Session, Listener, Transport,
};
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "openclipboard-e2e", version, about = "OpenClipboard E2E harness")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Listen on a port, accept one connection, and wait for a text clipboard event.
    Listen {
        /// Bind address, e.g. 127.0.0.1:18455
        #[arg(long)]
        bind: String,
        /// How long to wait for the clip text message (ms)
        #[arg(long, default_value_t = 15000)]
        timeout_ms: u64,
    },

    /// Connect to an address and send a text clipboard event.
    SendText {
        /// Target address, e.g. 127.0.0.1:18455
        #[arg(long)]
        addr: String,
        #[arg(long)]
        text: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.cmd {
        Cmd::Listen { bind, timeout_ms } => listen(bind, timeout_ms).await,
        Cmd::SendText { addr, text } => send_text(addr, text).await,
    }
}

async fn listen(bind: String, timeout_ms: u64) -> anyhow::Result<()> {
    let bind_addr: SocketAddr = bind.parse().context("parse --bind")?;
    let (endpoint, _cert) = make_server_endpoint(bind_addr).context("make_server_endpoint")?;
    let listener = QuicListener::new(endpoint);

    // Accept exactly one connection and then exit after receiving the expected message.
    let conn = tokio::time::timeout(Duration::from_millis(timeout_ms), listener.accept())
        .await
        .context("accept timeout")??;

    let identity = Ed25519Identity::generate();
    let clipboard = MockClipboard::new();
    let session = Session::new(conn, identity, clipboard);

    // Handshake is required to ensure the connection is authenticated at the application layer.
    let peer_id = session.handshake_with_timeout(Duration::from_millis(timeout_ms)).await?;

    // Wait for the first ClipText and print a JSON line.
    let msg = tokio::time::timeout(Duration::from_millis(timeout_ms), session.recv_message())
        .await
        .context("recv timeout")??;

    match msg {
        Message::ClipText { text, ts_ms, .. } => {
            let out = serde_json::json!({
                "type": "clip_text",
                "peer_id": peer_id,
                "text": text,
                "ts_ms": ts_ms,
            });
            println!("{}", out);
            Ok(())
        }
        other => {
            anyhow::bail!("expected ClipText, got {:?}", other.msg_type());
        }
    }
}

async fn send_text(addr: String, text: String) -> anyhow::Result<()> {
    let endpoint = make_insecure_client_endpoint().context("make_insecure_client_endpoint")?;
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr).await.context("connect")?;

    let identity = Ed25519Identity::generate();
    let clipboard = MockClipboard::new();
    let session = Session::new(conn, identity, clipboard);

    session.handshake_with_timeout(Duration::from_secs(15)).await?;

    session.clipboard.write(ClipboardContent::Text(text))?;
    session.send_clipboard().await?;

    // Give the receiver a moment to process before we drop the connection.
    tokio::time::sleep(Duration::from_millis(200)).await;

    Ok(())
}
