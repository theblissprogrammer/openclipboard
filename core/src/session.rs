//! Session manager: ties identity, transport, clipboard, and trust together.

use crate::clipboard::{ClipboardContent, ClipboardProvider};
use crate::identity::{Ed25519Identity, IdentityProvider};
use crate::protocol::{hello_transcript, Frame, Message};
use crate::replay::ReplayProtector;
use crate::transport::Connection;
use crate::trust::TrustStore;
use anyhow::Result;
use base64::Engine as _;
use rand_core::RngCore;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct Session<C: Connection, I: IdentityProvider, CB: ClipboardProvider> {
    pub conn: Arc<C>,
    pub identity: Arc<I>,
    pub clipboard: Arc<CB>,
    trust_store: Option<Arc<dyn TrustStore>>,
    replay: Option<Arc<dyn ReplayProtector>>,
    pairing_mode: bool,
    seq: AtomicU64,
}

impl<C: Connection, I: IdentityProvider, CB: ClipboardProvider> Session<C, I, CB> {
    pub fn new(conn: C, identity: I, clipboard: CB) -> Self {
        Self {
            conn: Arc::new(conn),
            identity: Arc::new(identity),
            clipboard: Arc::new(clipboard),
            trust_store: None,
            replay: None,
            pairing_mode: false,
            seq: AtomicU64::new(0),
        }
    }

    /// Create a session with trust verification.
    pub fn with_trust(conn: C, identity: I, clipboard: CB, trust_store: Arc<dyn TrustStore>) -> Self {
        Self {
            conn: Arc::new(conn),
            identity: Arc::new(identity),
            clipboard: Arc::new(clipboard),
            trust_store: Some(trust_store),
            replay: None,
            pairing_mode: false,
            seq: AtomicU64::new(0),
        }
    }

    /// Create a session with trust verification and optional replay protection.
    pub fn with_trust_and_replay(
        conn: C,
        identity: I,
        clipboard: CB,
        trust_store: Arc<dyn TrustStore>,
        replay: Arc<dyn ReplayProtector>,
    ) -> Self {
        Self {
            conn: Arc::new(conn),
            identity: Arc::new(identity),
            clipboard: Arc::new(clipboard),
            trust_store: Some(trust_store),
            replay: Some(replay),
            pairing_mode: false,
            seq: AtomicU64::new(0),
        }
    }

    /// Create a session in pairing mode (allows untrusted peers).
    pub fn with_pairing_mode(conn: C, identity: I, clipboard: CB, trust_store: Arc<dyn TrustStore>) -> Self {
        Self {
            conn: Arc::new(conn),
            identity: Arc::new(identity),
            clipboard: Arc::new(clipboard),
            trust_store: Some(trust_store),
            replay: None,
            pairing_mode: true,
            seq: AtomicU64::new(0),
        }
    }

    /// Create a session in pairing mode (allows untrusted peers), with optional replay protection.
    pub fn with_pairing_mode_and_replay(
        conn: C,
        identity: I,
        clipboard: CB,
        trust_store: Arc<dyn TrustStore>,
        replay: Arc<dyn ReplayProtector>,
    ) -> Self {
        Self {
            conn: Arc::new(conn),
            identity: Arc::new(identity),
            clipboard: Arc::new(clipboard),
            trust_store: Some(trust_store),
            replay: Some(replay),
            pairing_mode: true,
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn send_hello(&self) -> Result<()> {
        let peer_id = self.identity.peer_id().to_string();
        let version = crate::protocol::PROTOCOL_VERSION;
        let identity_pk = self.identity.public_key_bytes();

        let mut nonce = [0u8; 32];
        rand_core::OsRng.fill_bytes(&mut nonce);

        let transcript = hello_transcript(version, &peer_id, &identity_pk, &nonce);
        let sig = self.identity.sign(&transcript);

        let msg = Message::Hello {
            peer_id,
            version,
            identity_pk_b64: base64::engine::general_purpose::STANDARD.encode(&identity_pk),
            nonce_b64: base64::engine::general_purpose::STANDARD.encode(&nonce),
            sig_b64: base64::engine::general_purpose::STANDARD.encode(&sig),
        };
        self.send_message(&msg).await
    }

    /// Send HELLO and receive peer's HELLO, verifying trust.
    /// Returns the peer's peer_id on success.
    pub async fn handshake(&self) -> Result<String> {
        self.handshake_with_timeout(Duration::from_secs(5)).await
    }

    /// Handshake with an explicit timeout so we never hang forever.
    pub async fn handshake_with_timeout(&self, timeout_dur: Duration) -> Result<String> {
        // Send our HELLO
        self.send_hello().await?;

        // Receive peer's HELLO
        let frame = tokio::time::timeout(timeout_dur, self.conn.recv())
            .await
            .map_err(|_| anyhow::anyhow!("handshake timed out"))??;
        let msg: Message = serde_json::from_slice(&frame.payload)?;

        match msg {
            Message::Hello {
                peer_id,
                version,
                identity_pk_b64,
                nonce_b64,
                sig_b64,
            } => {
                let identity_pk = base64::engine::general_purpose::STANDARD.decode(&identity_pk_b64)?;
                let nonce = base64::engine::general_purpose::STANDARD.decode(&nonce_b64)?;
                let sig = base64::engine::general_purpose::STANDARD.decode(&sig_b64)?;

                if identity_pk.len() != 32 {
                    self.conn.close();
                    anyhow::bail!("invalid identity_pk length");
                }
                if nonce.len() != 32 {
                    self.conn.close();
                    anyhow::bail!("invalid nonce length");
                }
                if sig.len() != 64 {
                    self.conn.close();
                    anyhow::bail!("invalid signature length");
                }

                // Self-consistency: peer_id must be derived from the presented public key.
                let derived = Ed25519Identity::peer_id_from_public_key(&identity_pk);
                if derived != peer_id {
                    self.conn.close();
                    anyhow::bail!("peer_id/public_key mismatch");
                }

                // Verify proof-of-possession.
                let transcript = hello_transcript(version, &peer_id, &identity_pk, &nonce);
                if !Ed25519Identity::verify_with_public_key(&transcript, &sig, &identity_pk) {
                    self.conn.close();
                    anyhow::bail!("invalid hello signature");
                }

                // Optional anti-replay: after signature verification, reject reused nonces.
                if let Some(ref replay) = self.replay {
                    replay.check_and_store(&peer_id, &nonce)?;
                }

                // Check trust if trust store is configured and not in pairing mode.
                if let Some(ref store) = self.trust_store {
                    if !self.pairing_mode {
                        let Some(rec) = store.get(&peer_id)? else {
                            self.conn.close();
                            anyhow::bail!("untrusted peer: {}", peer_id);
                        };
                        if rec.identity_pk != identity_pk {
                            self.conn.close();
                            anyhow::bail!("trusted peer public key mismatch: {}", peer_id);
                        }
                    }
                }

                Ok(peer_id)
            }
            _ => {
                self.conn.close();
                anyhow::bail!("expected Hello message, got {:?}", msg.msg_type());
            }
        }
    }

    pub async fn send_clipboard(&self) -> Result<()> {
        let content = self.clipboard.read()?;
        let msg = match content {
            ClipboardContent::Empty => return Ok(()),
            ClipboardContent::Text(text) => Message::ClipText {
                mime: "text/plain".into(),
                text,
                ts_ms: now_ms(),
            },
            ClipboardContent::Image { mime, width, height, bytes } => Message::ClipImage {
                mime,
                width,
                height,
                bytes_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                ts_ms: now_ms(),
            },
        };
        self.send_message(&msg).await
    }

    pub async fn receive_clipboard(&self) -> Result<()> {
        let frame = self.conn.recv().await?;
        let payload: Message = serde_json::from_slice(&frame.payload)?;
        match payload {
            Message::ClipText { text, .. } => {
                self.clipboard.write(ClipboardContent::Text(text))?;
            }
            Message::ClipImage { mime, width, height, bytes_b64, .. } => {
                let bytes = base64::engine::general_purpose::STANDARD.decode(&bytes_b64)?;
                self.clipboard.write(ClipboardContent::Image { mime, width, height, bytes })?;
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn send_file_offer(&self, file_id: &str, name: &str, size: u64, mime: &str) -> Result<()> {
        let msg = Message::FileOffer {
            file_id: file_id.into(),
            name: name.into(),
            size,
            mime: mime.into(),
        };
        self.send_message(&msg).await
    }

    pub async fn send_file_accept(&self, file_id: &str) -> Result<()> {
        self.send_message(&Message::FileAccept { file_id: file_id.into() }).await
    }

    pub async fn send_file_chunk(&self, file_id: &str, offset: u64, data: &[u8]) -> Result<()> {
        let msg = Message::FileChunk {
            file_id: file_id.into(),
            offset,
            data_b64: base64::engine::general_purpose::STANDARD.encode(data),
        };
        self.send_message(&msg).await
    }

    pub async fn send_file_done(&self, file_id: &str, hash: &str) -> Result<()> {
        self.send_message(&Message::FileDone { file_id: file_id.into(), hash: hash.into() }).await
    }

    pub async fn recv_message(&self) -> Result<Message> {
        let frame = self.conn.recv().await?;
        let msg: Message = serde_json::from_slice(&frame.payload)?;
        Ok(msg)
    }

    async fn send_message(&self, msg: &Message) -> Result<()> {
        let payload = serde_json::to_vec(msg)?;
        let frame = Frame::new(msg.msg_type(), msg.stream_id(), self.next_seq(), payload);
        self.conn.send(frame).await
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::MockClipboard;
    use crate::identity::{Ed25519Identity, MockIdentity};
    use crate::replay::MemoryReplayProtector;
    use crate::transport::memory_connection_pair;
    use crate::trust::MemoryTrustStore;

    #[tokio::test]
    async fn send_receive_clipboard_text() {
        let (conn_a, conn_b) = memory_connection_pair();
        let cb_a = MockClipboard::new();
        cb_a.write(ClipboardContent::Text("hello world".into())).unwrap();
        let session_a = Session::new(conn_a, MockIdentity::new("a"), cb_a);

        let cb_b = MockClipboard::new();
        let session_b = Session::new(conn_b, MockIdentity::new("b"), cb_b);

        session_a.send_clipboard().await.unwrap();
        session_b.receive_clipboard().await.unwrap();

        assert_eq!(session_b.clipboard.read().unwrap(), ClipboardContent::Text("hello world".into()));
    }

    #[tokio::test]
    async fn send_hello() {
        let (conn_a, conn_b) = memory_connection_pair();
        let id = Ed25519Identity::generate();
        let expected_peer_id = id.peer_id().to_string();
        let expected_pk = id.public_key_bytes();
        let session_a = Session::new(conn_a, id, MockClipboard::new());

        session_a.send_hello().await.unwrap();
        let frame = conn_b.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&frame.payload).unwrap();
        match msg {
            Message::Hello {
                peer_id,
                version,
                identity_pk_b64,
                nonce_b64,
                sig_b64,
            } => {
                assert_eq!(version, crate::protocol::PROTOCOL_VERSION);
                assert_eq!(peer_id, expected_peer_id);

                let pk = base64::engine::general_purpose::STANDARD.decode(identity_pk_b64).unwrap();
                assert_eq!(pk, expected_pk);

                let nonce = base64::engine::general_purpose::STANDARD.decode(nonce_b64).unwrap();
                assert_eq!(nonce.len(), 32);

                let sig = base64::engine::general_purpose::STANDARD.decode(sig_b64).unwrap();
                assert_eq!(sig.len(), 64);
            }
            _ => panic!("expected Hello"),
        }
    }

    #[tokio::test]
    async fn empty_clipboard_noop() {
        let (conn_a, _conn_b) = memory_connection_pair();
        let session_a = Session::new(conn_a, MockIdentity::new("a"), MockClipboard::new());
        session_a.send_clipboard().await.unwrap();
    }

    fn make_signed_hello(
        signing_identity: &Ed25519Identity,
        claimed_peer_id: String,
        presented_pk: Vec<u8>,
        nonce: [u8; 32],
        sig_override: Option<Vec<u8>>,
    ) -> Message {
        let version = crate::protocol::PROTOCOL_VERSION;
        let transcript = hello_transcript(version, &claimed_peer_id, &presented_pk, &nonce);
        let sig = sig_override.unwrap_or_else(|| signing_identity.sign(&transcript));
        Message::Hello {
            peer_id: claimed_peer_id,
            version,
            identity_pk_b64: base64::engine::general_purpose::STANDARD.encode(&presented_pk),
            nonce_b64: base64::engine::general_purpose::STANDARD.encode(nonce),
            sig_b64: base64::engine::general_purpose::STANDARD.encode(&sig),
        }
    }

    #[tokio::test]
    async fn handshake_accept_trusted_peer_with_pinned_key() {
        let (conn_a, conn_b) = memory_connection_pair();

        let alice = Ed25519Identity::generate();
        let bob = Ed25519Identity::generate();

        let trust_a = Arc::new(MemoryTrustStore::new());
        trust_a
            .save(crate::trust::TrustRecord {
                peer_id: bob.peer_id().to_string(),
                identity_pk: bob.public_key_bytes(),
                display_name: "Bob".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let trust_b = Arc::new(MemoryTrustStore::new());
        trust_b
            .save(crate::trust::TrustRecord {
                peer_id: alice.peer_id().to_string(),
                identity_pk: alice.public_key_bytes(),
                display_name: "Alice".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let session_a = Session::with_trust(conn_a, alice, MockClipboard::new(), trust_a);
        let session_b = Session::with_trust(conn_b, bob, MockClipboard::new(), trust_b);

        let (result_a, result_b) = tokio::join!(session_a.handshake(), session_b.handshake());
        assert_eq!(result_a.unwrap(), session_b.identity.peer_id());
        assert_eq!(result_b.unwrap(), session_a.identity.peer_id());
    }

    #[tokio::test]
    async fn handshake_reject_spoofed_peer_id_with_different_public_key() {
        let (conn_a, conn_b) = memory_connection_pair();

        let alice = Ed25519Identity::generate();
        let victim = Ed25519Identity::generate();
        let attacker = Ed25519Identity::generate();

        // Alice trusts the victim's peer_id *and* public key.
        let trust_a = Arc::new(MemoryTrustStore::new());
        trust_a
            .save(crate::trust::TrustRecord {
                peer_id: victim.peer_id().to_string(),
                identity_pk: victim.public_key_bytes(),
                display_name: "Victim".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let session_a = Session::with_trust(conn_a, alice, MockClipboard::new(), trust_a);

        // Attacker sends a HELLO claiming victim's peer_id but presenting attacker's pk.
        let spoof_msg = make_signed_hello(
            &attacker,
            victim.peer_id().to_string(),
            attacker.public_key_bytes(),
            [9u8; 32],
            None,
        );

        let handle = tokio::spawn(async move {
            // Read Alice's hello.
            let _ = conn_b.recv().await.unwrap();
            // Send spoofed hello.
            let payload = serde_json::to_vec(&spoof_msg).unwrap();
            let frame = Frame::new(spoof_msg.msg_type(), spoof_msg.stream_id(), 1, payload);
            conn_b.send(frame).await.unwrap();
        });

        let res = session_a.handshake().await;
        assert!(res.is_err());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_reject_bad_signature() {
        let (conn_a, conn_b) = memory_connection_pair();

        let alice = Ed25519Identity::generate();
        let bob = Ed25519Identity::generate();

        let trust_a = Arc::new(MemoryTrustStore::new());
        trust_a
            .save(crate::trust::TrustRecord {
                peer_id: bob.peer_id().to_string(),
                identity_pk: bob.public_key_bytes(),
                display_name: "Bob".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let session_a = Session::with_trust(conn_a, alice, MockClipboard::new(), trust_a);

        // Correct peer_id + pk, but invalid signature.
        let bad_sig_msg = make_signed_hello(
            &bob,
            bob.peer_id().to_string(),
            bob.public_key_bytes(),
            [1u8; 32],
            Some(vec![0u8; 64]),
        );

        let handle = tokio::spawn(async move {
            let _ = conn_b.recv().await.unwrap();
            let payload = serde_json::to_vec(&bad_sig_msg).unwrap();
            let frame = Frame::new(bad_sig_msg.msg_type(), bad_sig_msg.stream_id(), 1, payload);
            conn_b.send(frame).await.unwrap();
        });

        let res = session_a.handshake().await;
        assert!(res.is_err());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_reject_untrusted_peer_when_not_pairing() {
        let (conn_a, conn_b) = memory_connection_pair();

        let alice = Ed25519Identity::generate();
        let bob = Ed25519Identity::generate();

        let trust_a = Arc::new(MemoryTrustStore::new());
        // Bob is not in Alice's trust store.

        let session_a = Session::with_trust(conn_a, alice, MockClipboard::new(), trust_a);

        let bob_hello = make_signed_hello(
            &bob,
            bob.peer_id().to_string(),
            bob.public_key_bytes(),
            [2u8; 32],
            None,
        );

        let handle = tokio::spawn(async move {
            let _ = conn_b.recv().await.unwrap();
            let payload = serde_json::to_vec(&bob_hello).unwrap();
            let frame = Frame::new(bob_hello.msg_type(), bob_hello.stream_id(), 1, payload);
            conn_b.send(frame).await.unwrap();
        });

        let res = session_a.handshake().await;
        assert!(res.is_err());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_pairing_mode_accepts_unknown_peer_with_valid_hello() {
        let (conn_a, conn_b) = memory_connection_pair();

        let alice = Ed25519Identity::generate();
        let bob = Ed25519Identity::generate();

        let trust_a = Arc::new(MemoryTrustStore::new());
        let session_a = Session::with_pairing_mode(conn_a, alice, MockClipboard::new(), trust_a);

        let bob_hello = make_signed_hello(
            &bob,
            bob.peer_id().to_string(),
            bob.public_key_bytes(),
            [3u8; 32],
            None,
        );

        let handle = tokio::spawn(async move {
            let _ = conn_b.recv().await.unwrap();
            let payload = serde_json::to_vec(&bob_hello).unwrap();
            let frame = Frame::new(bob_hello.msg_type(), bob_hello.stream_id(), 1, payload);
            conn_b.send(frame).await.unwrap();
        });

        let res = session_a.handshake().await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), bob.peer_id());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_rejects_replayed_hello_nonce_when_replay_protector_enabled() {
        let (conn_a, conn_b) = memory_connection_pair();

        let alice = Ed25519Identity::generate();
        let bob = Ed25519Identity::generate();

        let trust_a = Arc::new(MemoryTrustStore::new());
        let replay = Arc::new(MemoryReplayProtector::new(16));
        let session_a = Session::with_pairing_mode_and_replay(conn_a, alice, MockClipboard::new(), trust_a, replay);

        // Bob will replay the exact same HELLO twice (same pk + nonce + sig).
        let replayed_hello = make_signed_hello(
            &bob,
            bob.peer_id().to_string(),
            bob.public_key_bytes(),
            [4u8; 32],
            None,
        );

        let handle = tokio::spawn(async move {
            for _ in 0..2 {
                let _ = conn_b.recv().await.unwrap();
                let payload = serde_json::to_vec(&replayed_hello).unwrap();
                let frame = Frame::new(replayed_hello.msg_type(), replayed_hello.stream_id(), 1, payload);
                conn_b.send(frame).await.unwrap();
            }
        });

        // First handshake should succeed.
        assert!(session_a.handshake_with_timeout(Duration::from_millis(500)).await.is_ok());
        // Second should fail due to replay.
        assert!(session_a.handshake_with_timeout(Duration::from_millis(500)).await.is_err());

        handle.await.unwrap();
    }
}
