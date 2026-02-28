//! CI-oriented end-to-end harness for openclipboard.
//!
//! This is intentionally simple and runner-friendly:
//! - Runs on a single machine (GitHub Actions runner)
//! - Uses real QUIC transport + Session handshake + message framing
//! - Avoids BLE, OS clipboard APIs, or UI

use anyhow::Context;
use base64::Engine as _;
use clap::{Parser, Subcommand};
use openclipboard::bench;
use openclipboard_core::{
    ClipboardContent, ClipboardProvider, Ed25519Identity, Listener, Message, MockClipboard,
    Session, Transport,
    protocol::Frame,
    quic_transport::{
        QuicListener, QuicTransport, make_insecure_client_endpoint, make_server_endpoint,
    },
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(
    name = "openclipboard-e2e",
    version,
    about = "OpenClipboard E2E harness"
)]
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

    /// Benchmark one-way message latency over local QUIC loopback.
    BenchLatency {
        /// Bind address for the in-process listener (use port 0 for auto).
        #[arg(long, default_value = "127.0.0.1:0")]
        bind: String,
        /// Number of messages to send.
        #[arg(long, default_value_t = 1000)]
        n: u32,
        /// Message payload size in bytes (before base64 encoding). First 8 bytes are a timestamp.
        #[arg(long, default_value_t = 32)]
        size: usize,
        /// Timeout for connect/handshake/recv (ms)
        #[arg(long, default_value_t = 15000)]
        timeout_ms: u64,
    },

    /// Benchmark throughput over local QUIC loopback.
    BenchThroughput {
        /// Bind address for the in-process listener (use port 0 for auto).
        #[arg(long, default_value = "127.0.0.1:0")]
        bind: String,
        /// Total bytes to send (before base64 encoding).
        #[arg(long, default_value_t = 100 * 1024 * 1024)]
        total_bytes: u64,
        /// Chunk size in bytes (before base64 encoding).
        #[arg(long, default_value_t = 64 * 1024)]
        chunk_bytes: usize,
        /// Timeout for connect/handshake/recv (ms)
        #[arg(long, default_value_t = 15000)]
        timeout_ms: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.cmd {
        Cmd::Listen { bind, timeout_ms } => listen(bind, timeout_ms).await,
        Cmd::SendText { addr, text } => send_text(addr, text).await,
        Cmd::BenchLatency {
            bind,
            n,
            size,
            timeout_ms,
        } => bench_latency(bind, n, size, timeout_ms).await,
        Cmd::BenchThroughput {
            bind,
            total_bytes,
            chunk_bytes,
            timeout_ms,
        } => bench_throughput(bind, total_bytes, chunk_bytes, timeout_ms).await,
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
    let peer_id = session
        .handshake_with_timeout(Duration::from_millis(timeout_ms))
        .await?;

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
        other => anyhow::bail!("expected ClipText, got {:?}", other.msg_type()),
    }
}

async fn send_text(addr: String, text: String) -> anyhow::Result<()> {
    let endpoint = make_insecure_client_endpoint().context("make_insecure_client_endpoint")?;
    let transport = QuicTransport::new(endpoint);
    let conn = transport.connect(&addr).await.context("connect")?;

    let identity = Ed25519Identity::generate();
    let clipboard = MockClipboard::new();
    let session = Session::new(conn, identity, clipboard);

    session
        .handshake_with_timeout(Duration::from_secs(15))
        .await?;

    session.clipboard.write(ClipboardContent::Text(text))?;
    session.send_clipboard().await?;

    // Give the receiver a moment to process before we drop the connection.
    tokio::time::sleep(Duration::from_millis(200)).await;

    Ok(())
}

async fn bench_latency(bind: String, n: u32, size: usize, timeout_ms: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        size >= 8,
        "--size must be >= 8 (first 8 bytes are timestamp)"
    );

    let bind_addr: SocketAddr = bind.parse().context("parse --bind")?;

    let start = Instant::now();

    let (addr_tx, addr_rx) = std::sync::mpsc::channel::<SocketAddr>();
    let (res_tx, res_rx) = std::sync::mpsc::channel::<anyhow::Result<Vec<u128>>>();

    // Run the QUIC server side on a dedicated runtime/thread.
    let server_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");

        let res: anyhow::Result<Vec<u128>> = rt.block_on(async move {
            let (endpoint, _cert) =
                make_server_endpoint(bind_addr).context("make_server_endpoint")?;
            let server_addr = endpoint.local_addr().context("server local_addr")?;
            addr_tx.send(server_addr).expect("send server addr");

            let listener = QuicListener::new(endpoint);
            let conn = tokio::time::timeout(Duration::from_millis(timeout_ms), listener.accept())
                .await
                .context("accept timeout")??;

            let session = Session::new(conn, Ed25519Identity::generate(), MockClipboard::new());
            session
                .handshake_with_timeout(Duration::from_millis(timeout_ms))
                .await
                .context("handshake")?;

            let mut lats_ns: Vec<u128> = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let msg =
                    tokio::time::timeout(Duration::from_millis(timeout_ms), session.recv_message())
                        .await
                        .context("recv timeout")??;

                let Message::ClipText { text, .. } = msg else {
                    continue;
                };

                let bytes = base64::engine::general_purpose::STANDARD.decode(text)?;
                anyhow::ensure!(bytes.len() >= 8, "payload too small");
                let sent_ns = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
                let sent_at = start + Duration::from_nanos(sent_ns);
                lats_ns.push(Instant::now().duration_since(sent_at).as_nanos());
            }

            anyhow::Ok(lats_ns)
        });

        let _ = res_tx.send(res);
    });

    let server_addr = tokio::task::spawn_blocking(move || addr_rx.recv())
        .await
        .context("addr recv join")??;

    let endpoint = make_insecure_client_endpoint().context("make_insecure_client_endpoint")?;
    let transport = QuicTransport::new(endpoint);
    let conn = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        transport.connect(&server_addr.to_string()),
    )
    .await
    .context("connect timeout")??;

    let session = Session::new(conn, Ed25519Identity::generate(), MockClipboard::new());
    session
        .handshake_with_timeout(Duration::from_millis(timeout_ms))
        .await
        .context("handshake")?;

    let mut send_seq = AtomicU64::new(0);
    for i in 0..n {
        let mut payload = vec![0u8; size];
        let ns = start.elapsed().as_nanos() as u64;
        payload[0..8].copy_from_slice(&ns.to_be_bytes());
        if size > 8 {
            payload[8] = (i & 0xFF) as u8;
        }

        let msg = Message::ClipText {
            mime: "application/octet-stream".into(),
            text: base64::engine::general_purpose::STANDARD.encode(payload),
            ts_ms: 0,
        };
        send_msg(&session, &mut send_seq, msg).await?;
    }

    let lats_ns = tokio::task::spawn_blocking(move || res_rx.recv())
        .await
        .context("res recv join")??;

    let lats_ns = lats_ns?;

    let _ = server_thread.join();

    let lats_us: Vec<f64> = lats_ns.iter().map(|ns| *ns as f64 / 1_000.0).collect();
    let Some(sum) = bench::summarize(&lats_us) else {
        anyhow::bail!("no samples collected");
    };

    println!("latency_us: {sum}");
    Ok(())
}

async fn bench_throughput(
    bind: String,
    total_bytes: u64,
    chunk_bytes: usize,
    timeout_ms: u64,
) -> anyhow::Result<()> {
    anyhow::ensure!(chunk_bytes > 0, "--chunk-bytes must be > 0");

    let bind_addr: SocketAddr = bind.parse().context("parse --bind")?;

    let (addr_tx, addr_rx) = std::sync::mpsc::channel::<SocketAddr>();

    let server_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");

        rt.block_on(async move {
            let (endpoint, _cert) =
                make_server_endpoint(bind_addr).context("make_server_endpoint")?;
            let server_addr = endpoint.local_addr().context("server local_addr")?;
            addr_tx.send(server_addr).expect("send server addr");

            let listener = QuicListener::new(endpoint);
            let conn = tokio::time::timeout(Duration::from_millis(timeout_ms), listener.accept())
                .await
                .context("accept timeout")??;

            let session = Session::new(conn, Ed25519Identity::generate(), MockClipboard::new());
            session
                .handshake_with_timeout(Duration::from_millis(timeout_ms))
                .await
                .context("handshake")?;

            let mut recv_total: u64 = 0;
            while recv_total < total_bytes {
                let msg = session.recv_message().await?;
                if let Message::ClipText { text, .. } = msg {
                    let bytes = base64::engine::general_purpose::STANDARD.decode(text)?;
                    recv_total += bytes.len() as u64;
                }
            }

            anyhow::Ok(())
        })
    });

    let server_addr = tokio::task::spawn_blocking(move || addr_rx.recv())
        .await
        .context("addr recv join")??;

    let endpoint = make_insecure_client_endpoint().context("make_insecure_client_endpoint")?;
    let transport = QuicTransport::new(endpoint);
    let conn = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        transport.connect(&server_addr.to_string()),
    )
    .await
    .context("connect timeout")??;

    let session = Session::new(conn, Ed25519Identity::generate(), MockClipboard::new());
    session
        .handshake_with_timeout(Duration::from_millis(timeout_ms))
        .await
        .context("handshake")?;

    let mut send_seq = AtomicU64::new(0);

    let start = Instant::now();
    let mut sent: u64 = 0;
    while sent < total_bytes {
        let remaining = (total_bytes - sent) as usize;
        let sz = remaining.min(chunk_bytes);
        let payload = vec![0u8; sz];
        let msg = Message::ClipText {
            mime: "application/octet-stream".into(),
            text: base64::engine::general_purpose::STANDARD.encode(payload),
            ts_ms: 0,
        };
        send_msg(&session, &mut send_seq, msg).await?;
        sent += sz as u64;
    }

    let server_res = tokio::task::spawn_blocking(move || server_thread.join())
        .await
        .context("server join")?
        .map_err(|_| anyhow::anyhow!("server thread panicked"))?;
    server_res?;

    let elapsed = start.elapsed().as_secs_f64();
    let mb = total_bytes as f64 / (1024.0 * 1024.0);
    let mbps = mb / elapsed;

    println!(
        "throughput: total_bytes={} elapsed_s={:.6} MB/s={:.3}",
        total_bytes, elapsed, mbps
    );
    Ok(())
}

async fn send_msg<C, I, CB>(
    session: &Session<C, I, CB>,
    seq: &mut AtomicU64,
    msg: Message,
) -> anyhow::Result<()>
where
    C: openclipboard_core::Connection,
    I: openclipboard_core::IdentityProvider,
    CB: ClipboardProvider,
{
    let payload = serde_json::to_vec(&msg)?;
    let frame = Frame::new(
        msg.msg_type(),
        msg.stream_id(),
        seq.fetch_add(1, Ordering::SeqCst),
        payload,
    );
    session.conn.send(frame).await?;
    Ok(())
}
