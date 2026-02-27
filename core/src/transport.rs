//! Transport abstraction: Connection, Transport, Listener traits + MemoryTransport.

use crate::protocol::Frame;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[async_trait]
pub trait Connection: Send + Sync {
    async fn send(&self, frame: Frame) -> Result<()>;
    async fn recv(&self) -> Result<Frame>;
    /// Close the connection.
    fn close(&self);
    fn is_closed(&self) -> bool;
}

#[async_trait]
pub trait Transport: Send + Sync {
    type Conn: Connection;
    async fn connect(&self, addr: &str) -> Result<Self::Conn>;
}

#[async_trait]
pub trait Listener: Send + Sync {
    type Conn: Connection;
    async fn accept(&self) -> Result<Self::Conn>;
}

// ── MemoryTransport ──

/// Create a pair of connected in-memory connections.
pub fn memory_connection_pair() -> (MemoryConnection, MemoryConnection) {
    let (tx_a, rx_a) = mpsc::channel::<Frame>(64);
    let (tx_b, rx_b) = mpsc::channel::<Frame>(64);
    let a = MemoryConnection {
        tx: tx_a,
        rx: Arc::new(Mutex::new(rx_b)),
        closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let b = MemoryConnection {
        tx: tx_b,
        rx: Arc::new(Mutex::new(rx_a)),
        closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    (a, b)
}

pub struct MemoryConnection {
    tx: mpsc::Sender<Frame>,
    rx: Arc<Mutex<mpsc::Receiver<Frame>>>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl Connection for MemoryConnection {
    async fn send(&self, frame: Frame) -> Result<()> {
        if self.is_closed() {
            anyhow::bail!("connection closed");
        }
        self.tx.send(frame).await.map_err(|_| anyhow::anyhow!("send failed"))?;
        Ok(())
    }

    async fn recv(&self) -> Result<Frame> {
        let mut rx = self.rx.lock().await;
        rx.recv().await.ok_or_else(|| anyhow::anyhow!("connection closed"))
    }

    fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// MemoryListener accepts connections pushed to it.
pub struct MemoryListener {
    rx: Arc<Mutex<mpsc::Receiver<MemoryConnection>>>,
}

impl MemoryListener {
    pub fn new(rx: mpsc::Receiver<MemoryConnection>) -> Self {
        Self { rx: Arc::new(Mutex::new(rx)) }
    }
}

#[async_trait]
impl Listener for MemoryListener {
    type Conn = MemoryConnection;
    async fn accept(&self) -> Result<MemoryConnection> {
        let mut rx = self.rx.lock().await;
        rx.recv().await.ok_or_else(|| anyhow::anyhow!("listener closed"))
    }
}

/// Helper: create a MemoryTransport-like setup returning (client_connect_fn, listener).
pub fn memory_transport_pair() -> (mpsc::Sender<MemoryConnection>, MemoryListener) {
    let (tx, rx) = mpsc::channel::<MemoryConnection>(16);
    (tx, MemoryListener::new(rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{MsgType, StreamId};

    #[tokio::test]
    async fn memory_connection_send_recv() {
        let (a, b) = memory_connection_pair();
        let frame = Frame::new(MsgType::Ping, StreamId::Control, 1, b"test".to_vec());
        a.send(frame.clone()).await.unwrap();
        let got = b.recv().await.unwrap();
        assert_eq!(got, frame);
    }

    #[tokio::test]
    async fn memory_connection_bidirectional() {
        let (a, b) = memory_connection_pair();
        let f1 = Frame::new(MsgType::Ping, StreamId::Control, 1, b"ping".to_vec());
        let f2 = Frame::new(MsgType::Pong, StreamId::Control, 2, b"pong".to_vec());
        a.send(f1.clone()).await.unwrap();
        b.send(f2.clone()).await.unwrap();
        assert_eq!(b.recv().await.unwrap(), f1);
        assert_eq!(a.recv().await.unwrap(), f2);
    }

    #[tokio::test]
    async fn memory_connection_close() {
        let (a, _b) = memory_connection_pair();
        a.close();
        assert!(a.is_closed());
        let f = Frame::new(MsgType::Ping, StreamId::Control, 1, vec![]);
        assert!(a.send(f).await.is_err());
    }

    #[tokio::test]
    async fn memory_listener_accept() {
        let (tx, listener) = memory_transport_pair();
        let (_client, server) = memory_connection_pair();
        tx.send(server).await.unwrap();
        let accepted = listener.accept().await.unwrap();
        assert!(!accepted.is_closed());
    }
}
