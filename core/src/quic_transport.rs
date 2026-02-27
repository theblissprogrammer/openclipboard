//! QUIC transport implementation using quinn.

use crate::protocol::{decode_frame, encode_frame, Frame};
use crate::transport::{Connection, Listener, Transport};
use anyhow::Result;
use async_trait::async_trait;
use quinn::{Endpoint, RecvStream, SendStream};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A QUIC connection wrapping a single bidirectional stream.
pub struct QuicConnection {
    send: Arc<Mutex<SendStream>>,
    recv: Arc<Mutex<RecvStream>>,
    closed: Arc<AtomicBool>,
}

impl QuicConnection {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        Self {
            send: Arc::new(Mutex::new(send)),
            recv: Arc::new(Mutex::new(recv)),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl Connection for QuicConnection {
    async fn send(&self, frame: Frame) -> Result<()> {
        if self.is_closed() {
            anyhow::bail!("connection closed");
        }
        let bytes = encode_frame(&frame);
        let len = (bytes.len() as u32).to_be_bytes();
        let mut send = self.send.lock().await;
        send.write_all(&len).await?;
        send.write_all(&bytes).await?;
        Ok(())
    }

    async fn recv(&self) -> Result<Frame> {
        let mut recv = self.recv.lock().await;
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;
        decode_frame(&buf)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

/// QUIC listener that accepts incoming connections.
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    pub fn new(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }
}

#[async_trait]
impl Listener for QuicListener {
    type Conn = QuicConnection;

    async fn accept(&self) -> Result<QuicConnection> {
        let incoming = self.endpoint.accept().await
            .ok_or_else(|| anyhow::anyhow!("listener closed"))?;
        let conn = incoming.await?;
        let (send, recv) = conn.accept_bi().await?;
        Ok(QuicConnection::new(send, recv))
    }
}

/// QUIC transport for connecting to a QUIC server.
pub struct QuicTransport {
    endpoint: Endpoint,
}

impl QuicTransport {
    pub fn new(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }
}

#[async_trait]
impl Transport for QuicTransport {
    type Conn = QuicConnection;

    async fn connect(&self, addr: &str) -> Result<QuicConnection> {
        let socket_addr: SocketAddr = addr.parse()?;
        let conn = self.endpoint.connect(socket_addr, "localhost")?.await?;
        let (send, recv) = conn.open_bi().await?;
        Ok(QuicConnection::new(send, recv))
    }
}

/// Create a self-signed certificate and key for testing.
///
/// Installs the ring crypto provider if not already set.
pub fn self_signed_cert() -> Result<(rustls::pki_types::CertificateDer<'static>, rustls::pki_types::PrivateKeyDer<'static>)> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let kp = rcgen::KeyPair::generate()?;
    let params = rcgen::CertificateParams::new(vec!["localhost".into()])?;
    let cert_pem = params.self_signed(&kp)?;
    let cert_der = rustls::pki_types::CertificateDer::from(cert_pem.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(kp.serialize_der()));
    Ok((cert_der, key_der))
}

/// Create a server endpoint bound to the given address with self-signed certs.
pub fn make_server_endpoint(bind_addr: SocketAddr) -> Result<(Endpoint, rustls::pki_types::CertificateDer<'static>)> {
    let (cert, key) = self_signed_cert()?;
    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key)?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)?,
    ));
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, cert))
}

/// Create a client endpoint that trusts the given server certificate.
pub fn make_client_endpoint(server_cert: rustls::pki_types::CertificateDer<'static>) -> Result<Endpoint> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(server_cert)?;
    let client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)?,
    ));
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}
