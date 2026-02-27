use anyhow::Result;
use base64::Engine as _;
use clap::{Parser, Subcommand};
use chrono::Utc;
use openclipboard::{
    default_identity_path, default_trust_path, load_or_create_identity, load_identity,
    pairing_finalize, pairing_init_qr, pairing_respond_qr, preview, sanitize_filename, save_identity,
    send_file,
};
use openclipboard_core::{
    ClipboardContent, ClipboardProvider, Ed25519Identity, FileTrustStore, IdentityProvider, Listener,
    MemoryReplayProtector, Session, Transport, TrustStore,
};
use openclipboard_core::clipboard::MockClipboard;
use openclipboard_core::quic_transport::{
    make_insecure_client_endpoint, make_server_endpoint, QuicListener, QuicTransport,
};
use rand_core::RngCore;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "openclipboard", version, about = "OpenClipboard LAN prototype CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(name = "id:new")]
    IdNew {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    #[command(name = "id:show")]
    IdShow {
        #[arg(long)]
        path: Option<PathBuf>,
    },

    #[command(name = "pair:init")]
    PairInit {
        #[arg(long)]
        name: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        id_path: Option<PathBuf>,
    },

    #[command(name = "pair:respond")]
    PairRespond {
        #[arg(long)]
        qr: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        id_path: Option<PathBuf>,
    },

    #[command(name = "pair:finalize")]
    PairFinalize {
        #[arg(long)]
        init_qr: String,
        #[arg(long)]
        resp_qr: String,
        #[arg(long)]
        trust_path: Option<PathBuf>,
    },

    #[command(name = "serve")]
    Serve {
        #[arg(long)]
        port: u16,
        #[arg(long)]
        name: String,
        #[arg(long)]
        id_path: Option<PathBuf>,
        #[arg(long)]
        trust_path: Option<PathBuf>,
    },

    #[command(name = "send:text")]
    SendText {
        #[arg(long)]
        addr: String,
        #[arg(long)]
        text: String,
        #[arg(long)]
        id_path: Option<PathBuf>,
        #[arg(long)]
        trust_path: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        pairing_mode: bool,
    },

    #[command(name = "send:file")]
    SendFile {
        #[arg(long)]
        addr: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        id_path: Option<PathBuf>,
        #[arg(long)]
        trust_path: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        pairing_mode: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::IdNew { path } => {
            let path = path.unwrap_or_else(default_identity_path);
            let id = Ed25519Identity::generate();
            save_identity(&path, &id)?;
            println!("wrote identity: {}", path.display());
            println!("peer_id: {}", id.peer_id());
            println!(
                "pubkey_b64: {}",
                base64::engine::general_purpose::STANDARD.encode(id.public_key_bytes())
            );
        }
        Command::IdShow { path } => {
            let path = path.unwrap_or_else(default_identity_path);
            let id = load_identity(&path)?;
            println!("identity: {}", path.display());
            println!("peer_id: {}", id.peer_id());
            println!(
                "pubkey_b64: {}",
                base64::engine::general_purpose::STANDARD.encode(id.public_key_bytes())
            );
        }
        Command::PairInit { name, port, id_path } => {
            let id_path = id_path.unwrap_or_else(default_identity_path);
            let id = load_or_create_identity(&id_path)?;

            let mut nonce = [0u8; 32];
            rand_core::OsRng.fill_bytes(&mut nonce);

            let qr = pairing_init_qr(name, port, &id, nonce);
            println!("init_qr: {qr}");
            println!("note: waiting for responder payload to derive code");
        }
        Command::PairRespond { qr, name, port, id_path } => {
            let id_path = id_path.unwrap_or_else(default_identity_path);
            let id = load_or_create_identity(&id_path)?;

            let (resp_qr, code) = pairing_respond_qr(&qr, name, port, &id)?;
            println!("resp_qr: {resp_qr}");
            println!("code: {code}");
        }
        Command::PairFinalize { init_qr, resp_qr, trust_path } => {
            let trust_path = trust_path.unwrap_or_else(default_trust_path);
            let (code, records) = pairing_finalize(&init_qr, &resp_qr)?;

            eprintln!("confirmation code: {code}");
            eprintln!("init: {} ({})", records[0].display_name, records[0].peer_id);
            eprintln!("resp: {} ({})", records[1].display_name, records[1].peer_id);

            eprint!("Write trust records to {}? [y/N]: ", trust_path.display());
            io::stdout().flush().ok();
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            if line.trim().to_lowercase() != "y" {
                eprintln!("aborted");
                return Ok(());
            }

            let store = FileTrustStore::new(trust_path.clone())?;
            for rec in records {
                store.save(openclipboard_core::TrustRecord {
                    peer_id: rec.peer_id,
                    identity_pk: rec.identity_pk,
                    display_name: rec.display_name,
                    created_at: Utc::now(),
                })?;
            }
            println!("wrote trust store: {}", trust_path.display());
        }
        Command::Serve { port, name: _name, id_path, trust_path } => {
            let id_path = id_path.unwrap_or_else(default_identity_path);
            let trust_path = trust_path.unwrap_or_else(default_trust_path);
            let identity = load_or_create_identity(&id_path)?;
            let trust = Arc::new(FileTrustStore::new(trust_path.clone())?);
            let replay = Arc::new(MemoryReplayProtector::new(1024));

            let bind: SocketAddr = format!("0.0.0.0:{port}").parse()?;
            let (endpoint, _cert) = make_server_endpoint(bind)?;
            let listener = QuicListener::new(endpoint);
            println!(
                "listening on {} (trust: {})",
                listener.local_addr()?,
                trust_path.display()
            );

            let mut files: HashMap<String, IncomingFile> = HashMap::new();

            loop {
                let conn = listener.accept().await?;
                let session = Session::with_trust_and_replay(
                    conn,
                    identity.clone(),
                    MockClipboard::new(),
                    trust.clone(),
                    replay.clone(),
                );

                match session.handshake().await {
                    Ok(peer_id) => {
                        println!("trusted peer connected: {peer_id}");
                    }
                    Err(e) => {
                        eprintln!("handshake failed: {e:#}");
                        continue;
                    }
                }

                loop {
                    let msg = match session.recv_message().await {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("connection ended: {e:#}");
                            break;
                        }
                    };

                    match msg {
                        openclipboard_core::Message::ClipText { mime: _, text, ts_ms } => {
                            println!(
                                "clip:text ts_ms={ts_ms} bytes={} preview={:?}",
                                text.len(),
                                preview(&text)
                            );
                        }
                        openclipboard_core::Message::ClipImage {
                            mime,
                            width,
                            height,
                            bytes_b64,
                            ts_ms,
                        } => {
                            let bytes = base64::engine::general_purpose::STANDARD.decode(bytes_b64)?;
                            println!(
                                "clip:image ts_ms={ts_ms} mime={mime} {width}x{height} bytes={}",
                                bytes.len()
                            );
                        }
                        openclipboard_core::Message::FileOffer { file_id, name, size, mime } => {
                            println!("file:offer id={file_id} name={name} size={size} mime={mime}");
                            files.insert(
                                file_id.clone(),
                                IncomingFile { name, expected: size, buf: Vec::new() },
                            );
                            session.send_file_accept(&file_id).await.ok();
                        }
                        openclipboard_core::Message::FileAccept { file_id } => {
                            println!("file:accept id={file_id}");
                        }
                        openclipboard_core::Message::FileChunk { file_id, offset, data_b64 } => {
                            let data = base64::engine::general_purpose::STANDARD.decode(data_b64)?;
                            println!("file:chunk id={file_id} offset={offset} len={}", data.len());
                            if let Some(f) = files.get_mut(&file_id) {
                                f.buf.extend_from_slice(&data);
                            }
                        }
                        openclipboard_core::Message::FileDone { file_id, hash } => {
                            println!("file:done id={file_id} hash={hash}");
                            if let Some(f) = files.remove(&file_id) {
                                println!(
                                    "file:received name={} bytes={} expected={}",
                                    f.name,
                                    f.buf.len(),
                                    f.expected
                                );
                                let out_dir = PathBuf::from("received");
                                fs::create_dir_all(&out_dir).ok();
                                let out_path = out_dir.join(sanitize_filename(&f.name));
                                fs::write(&out_path, &f.buf).ok();
                                println!("file:written {}", out_path.display());
                            }
                        }
                        other => {
                            println!("msg: {:?}", other.msg_type());
                        }
                    }
                }
            }
        }
        Command::SendText { addr, text, id_path, trust_path, pairing_mode } => {
            let id_path = id_path.unwrap_or_else(default_identity_path);
            let trust_path = trust_path.unwrap_or_else(default_trust_path);
            let identity = load_or_create_identity(&id_path)?;
            let trust = Arc::new(FileTrustStore::new(trust_path.clone())?);
            let replay = Arc::new(MemoryReplayProtector::new(1024));

            let endpoint = make_insecure_client_endpoint()?;
            let transport = QuicTransport::new(endpoint);
            let conn = transport.connect(&addr).await?;

            let session = if pairing_mode {
                Session::with_pairing_mode_and_replay(conn, identity, MockClipboard::new(), trust, replay)
            } else {
                Session::with_trust_and_replay(conn, identity, MockClipboard::new(), trust, replay)
            };

            let peer = session.handshake().await?;
            println!("connected to {peer}");

            session.clipboard.write(ClipboardContent::Text(text))?;
            session.send_clipboard().await?;
            println!("sent clip:text");
        }
        Command::SendFile { addr, path, id_path, trust_path, pairing_mode } => {
            let id_path = id_path.unwrap_or_else(default_identity_path);
            let trust_path = trust_path.unwrap_or_else(default_trust_path);
            let identity = load_or_create_identity(&id_path)?;
            let trust = Arc::new(FileTrustStore::new(trust_path.clone())?);
            let replay = Arc::new(MemoryReplayProtector::new(1024));

            let endpoint = make_insecure_client_endpoint()?;
            let transport = QuicTransport::new(endpoint);
            let conn = transport.connect(&addr).await?;

            let session = if pairing_mode {
                Session::with_pairing_mode_and_replay(conn, identity, MockClipboard::new(), trust, replay)
            } else {
                Session::with_trust_and_replay(conn, identity, MockClipboard::new(), trust, replay)
            };

            let peer = session.handshake().await?;
            println!("connected to {peer}");

            send_file(&session, &path).await?;
            println!("sent file {}", path.display());
        }
    }

    Ok(())
}

struct IncomingFile {
    name: String,
    expected: u64,
    buf: Vec<u8>,
}
