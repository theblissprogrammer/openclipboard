use openclipboard_core::{Ed25519Identity, MockClipboard, Session};
use openclipboard_core::protocol::Frame;
use openclipboard_core::transport::Connection;

use anyhow::Result;
use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

#[derive(Clone, Debug)]
struct ChaosConfig {
    max_delay: Duration,
    /// Additional random delay up to this jitter.
    jitter: Duration,
    reorder_rate: f64,
    duplicate_rate: f64,
    drop_rate: f64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            max_delay: Duration::from_millis(0),
            jitter: Duration::from_millis(0),
            reorder_rate: 0.0,
            duplicate_rate: 0.0,
            drop_rate: 0.0,
        }
    }
}

fn schedule_send(tx: mpsc::Sender<Frame>, frame: Frame, delay: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if delay.as_millis() > 0 {
            tokio::time::sleep(delay).await;
        }
        let _ = tx.send(frame).await;
    })
}

/// A pair of endpoints connected via a fault-injecting router.
struct ChaosLink;

impl ChaosLink {
    fn pair(seed: u64, cfg: ChaosConfig) -> (ChaosConn, ChaosConn) {
        // Endpoint -> router
        let (a_out_tx, mut a_out_rx) = mpsc::channel::<Frame>(128);
        let (b_out_tx, mut b_out_rx) = mpsc::channel::<Frame>(128);

        // Router -> endpoint
        let (a_in_tx, a_in_rx) = mpsc::channel::<Frame>(128);
        let (b_in_tx, b_in_rx) = mpsc::channel::<Frame>(128);

        let closed = Arc::new(AtomicBool::new(false));

        let router_closed = closed.clone();
        tokio::spawn(async move {
            let mut rng = StdRng::seed_from_u64(seed);

            // Simple pending queues to enable occasional reordering.
            let mut pend_a_to_b: VecDeque<Frame> = Default::default();
            let mut pend_b_to_a: VecDeque<Frame> = Default::default();

            loop {
                if router_closed.load(Ordering::SeqCst) {
                    break;
                }

                tokio::select! {
                    v = a_out_rx.recv() => {
                        let Some(frame) = v else { break; };
                        handle_dir(&mut rng, &cfg, frame, &mut pend_a_to_b, b_in_tx.clone()).await;
                    }
                    v = b_out_rx.recv() => {
                        let Some(frame) = v else { break; };
                        handle_dir(&mut rng, &cfg, frame, &mut pend_b_to_a, a_in_tx.clone()).await;
                    }
                }

                // Opportunistically flush pending queues.
                flush_pending(&mut rng, &cfg, &mut pend_a_to_b, b_in_tx.clone()).await;
                flush_pending(&mut rng, &cfg, &mut pend_b_to_a, a_in_tx.clone()).await;
            }
        });

        let a = ChaosConn {
            tx: a_out_tx,
            rx: Mutex::new(a_in_rx),
            closed: closed.clone(),
        };
        let b = ChaosConn {
            tx: b_out_tx,
            rx: Mutex::new(b_in_rx),
            closed,
        };
        (a, b)
    }
}

async fn handle_dir(
    rng: &mut StdRng,
    cfg: &ChaosConfig,
    frame: Frame,
    pending: &mut VecDeque<Frame>,
    tx: mpsc::Sender<Frame>,
) {
    // Drop
    if cfg.drop_rate > 0.0 && rng.gen_bool(cfg.drop_rate.clamp(0.0, 1.0)) {
        return;
    }

    // Reorder: sometimes hold in pending instead of sending immediately.
    if cfg.reorder_rate > 0.0 && rng.gen_bool(cfg.reorder_rate.clamp(0.0, 1.0)) {
        pending.push_front(frame);
        return;
    }

    pending.push_back(frame);
    flush_pending(rng, cfg, pending, tx).await;
}

async fn flush_pending(
    rng: &mut StdRng,
    cfg: &ChaosConfig,
    pending: &mut VecDeque<Frame>,
    tx: mpsc::Sender<Frame>,
) {
    while let Some(frame) = pending.pop_front() {
        let base = cfg.max_delay.as_millis() as u64;
        let jit = cfg.jitter.as_millis() as u64;
        let delay_ms = if base == 0 && jit == 0 {
            0
        } else {
            base + if jit > 0 { rng.gen_range(0..=jit) } else { 0 }
        };
        let delay = Duration::from_millis(delay_ms);

        let _ = schedule_send(tx.clone(), frame.clone(), delay);

        if cfg.duplicate_rate > 0.0 && rng.gen_bool(cfg.duplicate_rate.clamp(0.0, 1.0)) {
            let _ = schedule_send(tx.clone(), frame, delay);
        }

        // Don't flush entire queue when delays are enabled.
        if delay_ms > 0 {
            break;
        }
    }
}

struct ChaosConn {
    tx: mpsc::Sender<Frame>,
    rx: Mutex<mpsc::Receiver<Frame>>,
    closed: Arc<AtomicBool>,
}

#[async_trait]
impl Connection for ChaosConn {
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
        self.closed.store(true, Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn chaos_delay_jitter_no_drop_handshake_succeeds() {
    let cfg = ChaosConfig {
        max_delay: Duration::from_millis(10),
        jitter: Duration::from_millis(10),
        ..Default::default()
    };
    let (conn_a, conn_b) = ChaosLink::pair(1234, cfg);

    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();

    let a = Session::new(conn_a, alice, MockClipboard::new());
    let b = Session::new(conn_b, bob, MockClipboard::new());

    let (ra, rb) = tokio::join!(
        a.handshake_with_timeout(Duration::from_secs(2)),
        b.handshake_with_timeout(Duration::from_secs(2))
    );

    assert!(ra.is_ok());
    assert!(rb.is_ok());
}

#[tokio::test]
async fn chaos_reorder_and_duplicate_no_drop_handshake_succeeds() {
    let cfg = ChaosConfig {
        reorder_rate: 0.5,
        duplicate_rate: 0.5,
        max_delay: Duration::from_millis(5),
        jitter: Duration::from_millis(5),
        ..Default::default()
    };
    let (conn_a, conn_b) = ChaosLink::pair(42, cfg);

    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();

    let a = Session::new(conn_a, alice, MockClipboard::new());
    let b = Session::new(conn_b, bob, MockClipboard::new());

    let (ra, rb) = tokio::join!(
        a.handshake_with_timeout(Duration::from_secs(2)),
        b.handshake_with_timeout(Duration::from_secs(2))
    );

    assert!(ra.is_ok());
    assert!(rb.is_ok());
}

#[tokio::test]
async fn chaos_drop_causes_handshake_timeout_not_hang() {
    let cfg = ChaosConfig { drop_rate: 1.0, ..Default::default() };
    let (conn_a, conn_b) = ChaosLink::pair(999, cfg);

    let alice = Ed25519Identity::generate();
    let bob = Ed25519Identity::generate();

    let a = Session::new(conn_a, alice, MockClipboard::new());
    let b = Session::new(conn_b, bob, MockClipboard::new());

    let (ra, rb) = tokio::join!(
        a.handshake_with_timeout(Duration::from_millis(200)),
        b.handshake_with_timeout(Duration::from_millis(200))
    );

    assert!(ra.is_err());
    assert!(rb.is_err());
}
