use anyhow::{Context, Result};
use base64::Engine as _;
use clap::{Parser, Subcommand};
use chrono::Utc;
use openclipboard_core::{
    derive_confirmation_code, ClipboardContent, ClipboardProvider, Ed25519Identity, FileTrustStore,
    IdentityProvider, Listener, MemoryReplayProtector, PairingPayload, Session, Transport, TrustStore,
};
use openclipboard_core::clipboard::MockClipboard;
use openclipboard_core::quic_transport::{make_insecure_client_endpoint, make_server_endpoint, QuicListener, QuicTransport};
use std::collections::HashMap;
use rand_core::RngCore;
use std::fs;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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

#[derive(serde::Serialize, serde::Deserialize)]
struct IdentityFile {
    /// base64 secret key bytes (ed25519 signing key seed)
    signing_key_b64: String,
}

fn default_identity_path() -> PathBuf {
    home_dir().join(".openclipboard").join("identity.json")
}

fn default_trust_path() -> PathBuf {
    // Prefer core helper if added later.
    home_dir().join(".openclipboard").join("trust.json")
}

fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    // Fallback: current dir.
    PathBuf::from(".")
}

fn load_or_create_identity(path: &Path) -> Result<Ed25519Identity> {
    if path.exists() {
        load_identity(path)
    } else {
        let id = Ed25519Identity::generate();
        save_identity(path, &id)?;
        Ok(id)
    }
}

fn load_identity(path: &Path) -> Result<Ed25519Identity> {
    let s = fs::read_to_string(path).with_context(|| format!("read identity file {}", path.display()))?;
    let file: IdentityFile = serde_json::from_str(&s)?;
    let sk_bytes = base64::engine::general_purpose::STANDARD
        .decode(file.signing_key_b64)
        .context("decode signing_key_b64")?;
    let sk_arr: [u8; 32] = sk_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32 bytes signing key seed"))?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&sk_arr);
    Ok(Ed25519Identity::from_signing_key(signing_key))
}

fn save_identity(path: &Path, id: &Ed25519Identity) -> Result<()> {
    let sk_bytes = id.signing_key_seed_bytes();
    let file = IdentityFile {
        signing_key_b64: base64::engine::general_purpose::STANDARD.encode(sk_bytes),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&file)?)?;
    Ok(())
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

            let payload = PairingPayload {
                version: 1,
                peer_id: id.peer_id().to_string(),
                name,
                identity_pk: id.public_key_bytes(),
                lan_port: port,
                nonce: nonce.to_vec(),
            };
            let qr = payload.to_qr_string();
            println!("init_qr: {qr}");
            println!("note: waiting for responder payload to derive code");
        }
        Command::PairRespond { qr, name, port, id_path } => {
            let id_path = id_path.unwrap_or_else(default_identity_path);
            let id = load_or_create_identity(&id_path)?;
            let init = PairingPayload::from_qr_string(&qr)?;

            let resp = PairingPayload {
                version: 1,
                peer_id: id.peer_id().to_string(),
                name,
                identity_pk: id.public_key_bytes(),
                lan_port: port,
                nonce: init.nonce.clone(),
            };
            let resp_qr = resp.to_qr_string();
            let code = derive_confirmation_code(&init.nonce, &init.peer_id, &resp.peer_id);
            println!("resp_qr: {resp_qr}");
            println!("code: {code}");
        }
        Command::PairFinalize { init_qr, resp_qr, trust_path } => {
            let trust_path = trust_path.unwrap_or_else(default_trust_path);
            let init = PairingPayload::from_qr_string(&init_qr)?;
            let resp = PairingPayload::from_qr_string(&resp_qr)?;

            if init.nonce != resp.nonce {
                anyhow::bail!("nonce mismatch between init and resp payload");
            }
            let code = derive_confirmation_code(&init.nonce, &init.peer_id, &resp.peer_id);
            eprintln!("confirmation code: {code}");
            eprintln!("init: {} ({})", init.name, init.peer_id);
            eprintln!("resp: {} ({})", resp.name, resp.peer_id);

            eprint!("Write trust records to {}? [y/N]: ", trust_path.display());
            io::stdout().flush().ok();
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            if line.trim().to_lowercase() != "y" {
                eprintln!("aborted");
                return Ok(());
            }

            let store = FileTrustStore::new(trust_path.clone())?;
            store.save(openclipboard_core::TrustRecord {
                peer_id: init.peer_id.clone(),
                identity_pk: init.identity_pk.clone(),
                display_name: init.name.clone(),
                created_at: Utc::now(),
            })?;
            store.save(openclipboard_core::TrustRecord {
                peer_id: resp.peer_id.clone(),
                identity_pk: resp.identity_pk.clone(),
                display_name: resp.name.clone(),
                created_at: Utc::now(),
            })?;
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
            println!("listening on {} (trust: {})", listener.local_addr()?, trust_path.display());

            // Basic receiver state for files.
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
                            println!("clip:text ts_ms={ts_ms} bytes={} preview={:?}", text.len(), preview(&text));
                        }
                        openclipboard_core::Message::ClipImage { mime, width, height, bytes_b64, ts_ms } => {
                            let bytes = base64::engine::general_purpose::STANDARD.decode(bytes_b64)?;
                            println!("clip:image ts_ms={ts_ms} mime={mime} {width}x{height} bytes={}", bytes.len());
                        }
                        openclipboard_core::Message::FileOffer { file_id, name, size, mime } => {
                            println!("file:offer id={file_id} name={name} size={size} mime={mime}");
                            files.insert(file_id.clone(), IncomingFile { name, expected: size, buf: Vec::new() });
                            // Accept immediately.
                            session.send_file_accept(&file_id).await.ok();
                        }
                        openclipboard_core::Message::FileAccept { file_id } => {
                            println!("file:accept id={file_id}");
                        }
                        openclipboard_core::Message::FileChunk { file_id, offset, data_b64 } => {
                            let data = base64::engine::general_purpose::STANDARD.decode(data_b64)?;
                            println!("file:chunk id={file_id} offset={offset} len={}", data.len());
                            if let Some(f) = files.get_mut(&file_id) {
                                // naive append; assumes in-order.
                                f.buf.extend_from_slice(&data);
                            }
                        }
                        openclipboard_core::Message::FileDone { file_id, hash } => {
                            println!("file:done id={file_id} hash={hash}");
                            if let Some(f) = files.remove(&file_id) {
                                println!("file:received name={} bytes={} expected={}", f.name, f.buf.len(), f.expected);
                                // Write to ./received/<name>
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

            // Use the session's clipboard helper to ensure proper framing/sequence.
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

fn preview(s: &str) -> String {
    const N: usize = 80;
    if s.len() <= N {
        return s.to_string();
    }
    format!("{}…", &s[..N])
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

struct IncomingFile {
    name: String,
    expected: u64,
    buf: Vec<u8>,
}

async fn send_file<C, I, CB>(session: &Session<C, I, CB>, path: &Path) -> Result<()>
where
    C: openclipboard_core::Connection,
    I: openclipboard_core::IdentityProvider,
    CB: openclipboard_core::ClipboardProvider,
{
    const CHUNK: usize = 64 * 1024;

    let data = fs::read(path).with_context(|| format!("read file {}", path.display()))?;
    let size = data.len() as u64;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.bin");

    let file_id = blake3::hash(format!("{}:{}", name, size).as_bytes()).to_hex().to_string();

    session
        .send_file_offer(&file_id, name, size, "application/octet-stream")
        .await?;

    // Wait a short time for accept, but don't require it.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), session.recv_message()).await;

    let mut offset = 0u64;
    for chunk in data.chunks(CHUNK) {
        session.send_file_chunk(&file_id, offset, chunk).await?;
        offset += chunk.len() as u64;
    }

    let hash = blake3::hash(&data).to_hex().to_string();
    session.send_file_done(&file_id, &hash).await?;
    Ok(())
}
