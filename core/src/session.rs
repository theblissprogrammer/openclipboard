//! Session manager: ties identity, transport, and clipboard together.

use crate::clipboard::{ClipboardContent, ClipboardProvider};
use crate::identity::IdentityProvider;
use crate::protocol::{Frame, Message};
use crate::transport::Connection;
use anyhow::Result;
use base64::Engine as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Session<C: Connection, I: IdentityProvider, CB: ClipboardProvider> {
    pub conn: Arc<C>,
    pub identity: Arc<I>,
    pub clipboard: Arc<CB>,
    seq: AtomicU64,
}

impl<C: Connection, I: IdentityProvider, CB: ClipboardProvider> Session<C, I, CB> {
    pub fn new(conn: C, identity: I, clipboard: CB) -> Self {
        Self {
            conn: Arc::new(conn),
            identity: Arc::new(identity),
            clipboard: Arc::new(clipboard),
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn send_hello(&self) -> Result<()> {
        let msg = Message::Hello {
            peer_id: self.identity.peer_id().to_string(),
            version: crate::protocol::PROTOCOL_VERSION,
        };
        self.send_message(&msg).await
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
    use crate::identity::MockIdentity;
    use crate::transport::memory_connection_pair;

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
        let session_a = Session::new(conn_a, MockIdentity::new("peer-a"), MockClipboard::new());

        session_a.send_hello().await.unwrap();
        let frame = conn_b.recv().await.unwrap();
        let msg: Message = serde_json::from_slice(&frame.payload).unwrap();
        match msg {
            Message::Hello { peer_id, .. } => assert_eq!(peer_id, "peer-a"),
            _ => panic!("expected Hello"),
        }
    }

    #[tokio::test]
    async fn empty_clipboard_noop() {
        let (conn_a, _conn_b) = memory_connection_pair();
        let session_a = Session::new(conn_a, MockIdentity::new("a"), MockClipboard::new());
        // Should not send anything for empty clipboard
        session_a.send_clipboard().await.unwrap();
    }
}
