//! Persistent peer connections + clipboard sync.

use crate::clipboard::{ClipboardContent, ClipboardProvider};
use crate::discovery::{Discovery, PeerInfo};
use crate::history::ClipboardHistory;
use crate::identity::Ed25519Identity;
use crate::identity::IdentityProvider;
use crate::mesh::PeerRegistry;
use crate::quic_transport::{make_insecure_client_endpoint, make_server_endpoint, QuicListener, QuicTransport};
use crate::replay::MemoryReplayProtector;
use crate::session::Session;
use crate::trust::TrustStore;
use crate::Message;
use crate::transport::{Listener, Transport};
use crate::transport::Connection;
use anyhow::{Context, Result};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};
use tokio::task::JoinHandle;

/// Callbacks invoked by the sync service.
pub trait SyncHandler: Send + Sync {
    fn on_clipboard_text(&self, peer_id: String, text: String, ts_ms: u64);
    fn on_peer_connected(&self, peer_id: String);
    fn on_peer_disconnected(&self, peer_id: String);
    fn on_error(&self, message: String);
}

/// Track recent clipboard contents written due to remote updates.
///
/// Used for echo suppression: if a remote write triggers a local clipboard-change event,
/// the platform can check `should_ignore_local_change`.
#[derive(Debug)]
pub struct EchoSuppressor {
    cap: usize,
    recent: VecDeque<String>,
}

impl EchoSuppressor {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            recent: VecDeque::new(),
        }
    }

    pub fn note_remote_write(&mut self, text: &str) {
        if self.recent.back().is_some_and(|t| t == text) {
            return;
        }
        self.recent.push_back(text.to_string());
        while self.recent.len() > self.cap {
            self.recent.pop_front();
        }
    }

    pub fn should_ignore_local_change(&self, text: &str) -> bool {
        self.recent.iter().any(|t| t == text)
    }
}

#[derive(Debug, Clone)]
struct Backoff {
    cur_ms: u64,
    max_ms: u64,
}

impl Backoff {
    fn new() -> Self {
        Self { cur_ms: 200, max_ms: 5_000 }
    }

    fn reset(&mut self) {
        self.cur_ms = 200;
    }

    fn next_delay(&mut self) -> std::time::Duration {
        let d = std::time::Duration::from_millis(self.cur_ms);
        self.cur_ms = (self.cur_ms * 2).min(self.max_ms);
        d
    }
}

struct PeerHandle {
    outbound_tx: mpsc::Sender<String>,
}

/// Persistent sync service: listens for incoming peers, dials discovered trusted peers,
/// and broadcasts clipboard text to all connected peers.
///
/// This is a LAN prototype: QUIC cert validation is disabled and we rely on the
/// application-layer session handshake + pinned public keys in the trust store.
pub struct SyncService<D: Discovery + 'static> {
    identity: Ed25519Identity,
    trust_store: Arc<dyn TrustStore>,
    replay: Arc<MemoryReplayProtector>,
    discovery: Arc<D>,

    local_listen: SocketAddr,
    device_name: String,

    handler: Arc<dyn SyncHandler>,

    peers: Arc<Mutex<HashMap<String, PeerHandle>>>,
    peer_registry: PeerRegistry,
    echo_suppressor: Arc<Mutex<EchoSuppressor>>,
    history: Arc<ClipboardHistory>,

    /// When true, the next clipboard write is a "silent recall" and should not trigger fanout.
    silent_write: Arc<std::sync::atomic::AtomicBool>,

    stop_tx: watch::Sender<bool>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl<D: Discovery + 'static> SyncService<D> {
    pub fn new(
        identity: Ed25519Identity,
        trust_store: Arc<dyn TrustStore>,
        replay: Arc<MemoryReplayProtector>,
        discovery: Arc<D>,
        local_listen: SocketAddr,
        device_name: String,
        handler: Arc<dyn SyncHandler>,
    ) -> Result<Self> {
        let (stop_tx, _stop_rx) = watch::channel(false);
        Ok(Self {
            identity,
            trust_store,
            replay,
            discovery,
            local_listen,
            device_name,
            handler,
            peers: Arc::new(Mutex::new(HashMap::new())),
            peer_registry: PeerRegistry::new(),
            echo_suppressor: Arc::new(Mutex::new(EchoSuppressor::new(32))),
            history: Arc::new(ClipboardHistory::new(100)),
            silent_write: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stop_tx,
            tasks: Mutex::new(Vec::new()),
        })
    }

    pub async fn start(&self) -> Result<()> {
        let (listener, _cert) = make_server_endpoint(self.local_listen)
            .with_context(|| format!("bind listener {}", self.local_listen))?;
        let listener = QuicListener::new(listener);

        // Advertising / discovery
        let peer_info = PeerInfo {
            peer_id: self.identity.peer_id().to_string(),
            name: self.device_name.clone(),
            addr: listener.local_addr()?.to_string(),
        };
        // best-effort: if advertise fails, we still can run with direct connects.
        if let Err(e) = self.discovery.start_discovery(peer_info).await {
            self.handler.on_error(format!("discovery start failed: {e}"));
        }

        let mut stop_rx = self.stop_tx.subscribe();
        let handler = Arc::clone(&self.handler);
        let identity = self.identity.clone();
        let trust_store = Arc::clone(&self.trust_store);
        let replay = Arc::clone(&self.replay);
        let peers = Arc::clone(&self.peers);
        let echo_sup = Arc::clone(&self.echo_suppressor);
        let registry = self.peer_registry.clone();
        let history = Arc::clone(&self.history);

        // Incoming accept loop
        let incoming_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => { break; }
                    conn = listener.accept() => {
                        let conn = match conn {
                            Ok(c) => c,
                            Err(e) => {
                                handler.on_error(format!("accept failed: {e}"));
                                continue;
                            }
                        };

                        let handler2 = Arc::clone(&handler);
                        let identity2 = identity.clone();
                        let trust2 = Arc::clone(&trust_store);
                        let replay2 = Arc::clone(&replay);
                        let peers2 = Arc::clone(&peers);
                        let echo2 = Arc::clone(&echo_sup);
                        let registry2 = registry.clone();
                        let history2 = Arc::clone(&history);
                        tokio::spawn(async move {
                            if let Err(e) = handle_incoming_connection(conn, identity2, trust2, replay2, peers2, handler2, echo2, registry2, history2).await {
                                // already reported most errors
                                let _ = e;
                            }
                        });
                    }
                }
            }
        });

        // Outbound dial loop (poll discovery)
        let mut stop_rx2 = self.stop_tx.subscribe();
        let identity3 = self.identity.clone();
        let trust3 = Arc::clone(&self.trust_store);
        let replay3 = Arc::clone(&self.replay);
        let discovery3 = Arc::clone(&self.discovery);
        let peers3 = Arc::clone(&self.peers);
        let handler3 = Arc::clone(&self.handler);
        let echo3 = Arc::clone(&self.echo_suppressor);
        let registry3 = self.peer_registry.clone();
        let history3 = Arc::clone(&self.history);
        let dial_task = tokio::spawn(async move {
            let endpoint = match make_insecure_client_endpoint() {
                Ok(ep) => ep,
                Err(e) => {
                    handler3.on_error(format!("client endpoint failed: {e}"));
                    return;
                }
            };
            let endpoint = Arc::new(endpoint);

            loop {
                tokio::select! {
                    _ = stop_rx2.changed() => { break; }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {}
                }

                let scanned = match discovery3.scan().await {
                    Ok(v) => v,
                    Err(e) => {
                        handler3.on_error(format!("discovery scan failed: {e}"));
                        continue;
                    }
                };

                for peer in scanned {
                    if peer.peer_id == identity3.peer_id().to_string() {
                        continue;
                    }
                    // trust gate
                    match trust3.is_trusted(&peer.peer_id) {
                        Ok(true) => {}
                        Ok(false) => continue,
                        Err(e) => {
                            handler3.on_error(format!("trust check failed: {e}"));
                            continue;
                        }
                    }
                    // dial rule
                    if identity3.peer_id().to_string() >= peer.peer_id {
                        continue;
                    }
                    // already connected?
                    if peers3.lock().await.contains_key(&peer.peer_id) {
                        continue;
                    }

                    let identity4 = identity3.clone();
                    let trust4 = Arc::clone(&trust3);
                    let replay4 = Arc::clone(&replay3);
                    let peers4 = Arc::clone(&peers3);
                    let handler4 = Arc::clone(&handler3);
                    let echo4 = Arc::clone(&echo3);
                    let registry4 = registry3.clone();
                    let history4 = Arc::clone(&history3);
                    let transport2 = QuicTransport::new((*endpoint).clone());
                    tokio::spawn(async move {
                        if let Err(e) = connect_loop(peer, transport2, identity4, trust4, replay4, peers4, handler4, echo4, registry4, history4).await {
                            let _ = e;
                        }
                    });
                }
            }
        });

        let mut tasks = self.tasks.lock().await;
        tasks.push(incoming_task);
        tasks.push(dial_task);
        Ok(())
    }

    pub async fn stop(&self) {
        let _ = self.stop_tx.send(true);

        // stop discovery
        let discovery = Arc::clone(&self.discovery);
        tokio::spawn(async move {
            let _ = discovery.stop_discovery().await;
        });

        let mut tasks = self.tasks.lock().await;
        for t in tasks.drain(..) {
            t.abort();
        }

        self.peers.lock().await.clear();
    }

    pub async fn broadcast_clip_text(&self, text: String) {
        let peers = self.peers.lock().await;
        for (peer_id, h) in peers.iter() {
            let _ = h.outbound_tx.send(text.clone()).await;
            let _ = peer_id;
        }
    }

    /// Get a reference to the peer registry.
    pub fn peer_registry(&self) -> &PeerRegistry {
        &self.peer_registry
    }

    /// Get a reference to the echo suppressor.
    pub fn echo_suppressor(&self) -> &Arc<Mutex<EchoSuppressor>> {
        &self.echo_suppressor
    }

    /// Get a reference to the clipboard history.
    pub fn history(&self) -> &Arc<ClipboardHistory> {
        &self.history
    }

    /// Get a reference to the silent-write flag.
    pub fn silent_write_flag(&self) -> &Arc<std::sync::atomic::AtomicBool> {
        &self.silent_write
    }

    /// Start mesh mode: run a clipboard watcher in the background and auto-broadcast changes.
    ///
    /// This calls `start()` first (listener + discovery + outbound connections), then adds
    /// a clipboard polling loop that fans out changes to all connected peers.
    pub async fn start_mesh(
        &self,
        provider: Arc<dyn ClipboardProvider>,
        poll_interval: std::time::Duration,
    ) -> Result<()> {
        // Load trust store into peer registry.
        self.peer_registry.load_from_trust(self.trust_store.as_ref()).await?;

        // Start the normal sync (listener + discovery + dial).
        self.start().await?;

        // Start clipboard watcher.
        let stop_rx = self.stop_tx.subscribe();
        let echo_sup = Arc::clone(&self.echo_suppressor);
        let peers = Arc::clone(&self.peers);
        let watcher_history = Arc::clone(&self.history);
        let silent_flag = Arc::clone(&self.silent_write);

        let watcher = crate::mesh::start_clipboard_watcher(
            provider,
            echo_sup,
            poll_interval,
            stop_rx,
            move |content| {
                if let ClipboardContent::Text(text) = content {
                    // Check if this is a silent recall write — skip fanout if so.
                    if silent_flag.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        // Still record in history as local.
                        watcher_history.record(text, "local".into());
                        return;
                    }

                    // Record local clipboard change in history.
                    watcher_history.record(text.clone(), "local".into());

                    // Fan out to all connected peers (fire-and-forget from the watcher's perspective).
                    let peers = peers.clone();
                    let rt = tokio::runtime::Handle::try_current();
                    if let Ok(handle) = rt {
                        handle.spawn(async move {
                            let map = peers.lock().await;
                            for (_pid, h) in map.iter() {
                                let _ = h.outbound_tx.send(text.clone()).await;
                            }
                        });
                    }
                }
            },
        );

        self.tasks.lock().await.push(watcher);
        Ok(())
    }
}

async fn handle_incoming_connection(
    conn: crate::quic_transport::QuicConnection,
    identity: Ed25519Identity,
    trust_store: Arc<dyn TrustStore>,
    replay: Arc<MemoryReplayProtector>,
    peers: Arc<Mutex<HashMap<String, PeerHandle>>>,
    handler: Arc<dyn SyncHandler>,
    echo_suppressor: Arc<Mutex<EchoSuppressor>>,
    registry: PeerRegistry,
    history: Arc<ClipboardHistory>,
) -> Result<()> {
    let session = Session::with_trust_and_replay(
        conn,
        identity.clone(),
        crate::clipboard::MockClipboard::new(),
        trust_store.clone(),
        replay.clone(),
    );

    let peer_id = match session.handshake().await {
        Ok(p) => p,
        Err(e) => {
            handler.on_error(format!("incoming handshake failed: {e}"));
            return Ok(());
        }
    };

    // dedupe: if we're the dialer, prefer outbound
    let local_id = identity.peer_id().to_string();
    if local_id < peer_id {
        // We should be dialing; reject inbound to avoid duplicates.
        handler.on_peer_disconnected(peer_id);
        session.conn.close();
        return Ok(());
    }

    let (tx, rx) = mpsc::channel::<String>(32);
    {
        let mut map = peers.lock().await;
        if map.contains_key(&peer_id) {
            return Ok(());
        }
        map.insert(peer_id.clone(), PeerHandle { outbound_tx: tx });
    }

    registry.set_online(&peer_id, None).await;
    handler.on_peer_connected(peer_id.clone());

    let res = peer_message_loop(session, peer_id.clone(), rx, Arc::clone(&handler), Arc::clone(&echo_suppressor), Arc::clone(&history)).await;

    peers.lock().await.remove(&peer_id);
    registry.set_offline(&peer_id).await;
    handler.on_peer_disconnected(peer_id);

    res
}

async fn connect_loop(
    peer: PeerInfo,
    transport: QuicTransport,
    identity: Ed25519Identity,
    trust_store: Arc<dyn TrustStore>,
    replay: Arc<MemoryReplayProtector>,
    peers: Arc<Mutex<HashMap<String, PeerHandle>>>,
    handler: Arc<dyn SyncHandler>,
    echo_suppressor: Arc<Mutex<EchoSuppressor>>,
    registry: PeerRegistry,
    history: Arc<ClipboardHistory>,
) -> Result<()> {
    let mut backoff = Backoff::new();

    loop {
        // If already connected (race), stop.
        if peers.lock().await.contains_key(&peer.peer_id) {
            return Ok(());
        }

        let conn = match transport.connect(&peer.addr).await {
            Ok(c) => c,
            Err(e) => {
                let d = backoff.next_delay();
                handler.on_error(format!("dial {} failed: {e}; retrying in {:?}", peer.peer_id, d));
                tokio::time::sleep(d).await;
                continue;
            }
        };

        let session = Session::with_trust_and_replay(
            conn,
            identity.clone(),
            crate::clipboard::MockClipboard::new(),
            trust_store.clone(),
            replay.clone(),
        );

        let peer_id = match session.handshake().await {
            Ok(p) => p,
            Err(e) => {
                let d = backoff.next_delay();
                handler.on_error(format!("handshake {} failed: {e}; retrying in {:?}", peer.peer_id, d));
                tokio::time::sleep(d).await;
                continue;
            }
        };

        if peer_id != peer.peer_id {
            handler.on_error(format!("dialed {}, but handshake reported peer_id {}", peer.peer_id, peer_id));
        }

        backoff.reset();

        let (tx, rx) = mpsc::channel::<String>(32);
        {
            let mut map = peers.lock().await;
            if map.contains_key(&peer.peer_id) {
                // someone else connected while we were handshaking
                return Ok(());
            }
            map.insert(peer.peer_id.clone(), PeerHandle { outbound_tx: tx });
        }

        registry.set_online(&peer.peer_id, Some(peer.addr.clone())).await;
        handler.on_peer_connected(peer.peer_id.clone());

        let loop_res = peer_message_loop(session, peer.peer_id.clone(), rx, Arc::clone(&handler), Arc::clone(&echo_suppressor), Arc::clone(&history)).await;

        peers.lock().await.remove(&peer.peer_id);
        registry.set_offline(&peer.peer_id).await;
        handler.on_peer_disconnected(peer.peer_id.clone());

        let _ = loop_res;
        let d = backoff.next_delay();
        tokio::time::sleep(d).await;
    }
}

async fn peer_message_loop<C: crate::transport::Connection, I: crate::identity::IdentityProvider, P: ClipboardProvider>(
    session: Session<C, I, P>,
    peer_id: String,
    mut outbound_rx: mpsc::Receiver<String>,
    handler: Arc<dyn SyncHandler>,
    echo_suppressor: Arc<Mutex<EchoSuppressor>>,
    history: Arc<ClipboardHistory>,
) -> Result<()> {
    loop {
        tokio::select! {
            maybe_text = outbound_rx.recv() => {
                let Some(text) = maybe_text else { return Ok(()); };
                if let Err(e) = session.clipboard.write(crate::clipboard::ClipboardContent::Text(text)) {
                    handler.on_error(format!("clipboard write (for send to {peer_id}) failed: {e}"));
                    return Ok(());
                }
                if let Err(e) = session.send_clipboard().await {
                    handler.on_error(format!("send to {peer_id} failed: {e}"));
                    return Ok(());
                }
            }
            msg = session.recv_message() => {
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        handler.on_error(format!("recv from {peer_id} failed: {e}"));
                        return Ok(());
                    }
                };

                if let Message::ClipText { text, ts_ms, .. } = msg {
                    // Note in echo suppressor so the clipboard watcher won't re-broadcast.
                    echo_suppressor.lock().await.note_remote_write(&text);
                    // Record in history.
                    history.record(text.clone(), peer_id.clone());
                    handler.on_clipboard_text(peer_id.clone(), text, ts_ms);
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_suppressor_tracks_recent() {
        let mut s = EchoSuppressor::new(3);
        s.note_remote_write("a");
        s.note_remote_write("b");
        assert!(s.should_ignore_local_change("a"));
        assert!(!s.should_ignore_local_change("c"));
        s.note_remote_write("c");
        s.note_remote_write("d");
        // cap=3 -> a should be evicted
        assert!(!s.should_ignore_local_change("a"));
        assert!(s.should_ignore_local_change("b"));
        assert!(s.should_ignore_local_change("c"));
        assert!(s.should_ignore_local_change("d"));
    }

    #[test]
    fn backoff_grows_and_caps() {
        let mut b = Backoff { cur_ms: 200, max_ms: 500 };
        assert_eq!(b.next_delay(), std::time::Duration::from_millis(200));
        assert_eq!(b.next_delay(), std::time::Duration::from_millis(400));
        assert_eq!(b.next_delay(), std::time::Duration::from_millis(500));
        assert_eq!(b.next_delay(), std::time::Duration::from_millis(500));
        b.reset();
        assert_eq!(b.next_delay(), std::time::Duration::from_millis(200));
    }
}
