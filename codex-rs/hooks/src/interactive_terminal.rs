//! Process-local handoff between a synchronous interactive hook and the embedded TUI.
//!
//! Hook stdin/stdout remain pipes for the JSON protocol. The human-facing program opens the
//! controlling terminal separately, so the embedded TUI must stop reading and drawing before the
//! hook is spawned. This broker provides that ready barrier without pretending a best-effort hook
//! lifecycle notification is an ownership acknowledgement.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;

use thiserror::Error;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug)]
pub struct InteractiveTerminalRequest {
    pub lease_id: String,
    pub ready: oneshot::Sender<Result<(), String>>,
    pub finished: oneshot::Receiver<()>,
}

#[derive(Clone)]
struct OwnerRoute {
    generation: Uuid,
    sender: mpsc::UnboundedSender<InteractiveTerminalRequest>,
}

fn owner_route() -> &'static Mutex<Option<OwnerRoute>> {
    static OWNER_ROUTE: OnceLock<Mutex<Option<OwnerRoute>>> = OnceLock::new();
    OWNER_ROUTE.get_or_init(|| Mutex::new(None))
}

fn lock_owner_route() -> MutexGuard<'static, Option<OwnerRoute>> {
    owner_route()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn lease_semaphore() -> &'static Arc<Semaphore> {
    static LEASE_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LEASE_SEMAPHORE.get_or_init(|| Arc::new(Semaphore::new(1)))
}

/// The single terminal owner registered by an embedded Codex TUI event loop.
pub struct InteractiveTerminalOwner {
    generation: Uuid,
    receiver: mpsc::UnboundedReceiver<InteractiveTerminalRequest>,
}

impl InteractiveTerminalOwner {
    pub async fn recv(&mut self) -> Option<InteractiveTerminalRequest> {
        self.receiver.recv().await
    }
}

impl Drop for InteractiveTerminalOwner {
    fn drop(&mut self) {
        let mut route = lock_owner_route();
        if route
            .as_ref()
            .is_some_and(|route| route.generation == self.generation)
        {
            *route = None;
        }
    }
}

/// Register the process-local terminal owner for the lifetime of the returned receiver.
///
/// Registration is intentionally performed only by the embedded TUI. Remote app-server clients,
/// desktop clients, and `codex exec` do not own the server process's controlling terminal.
pub fn register_owner() -> InteractiveTerminalOwner {
    let generation = Uuid::new_v4();
    let (sender, receiver) = mpsc::unbounded_channel();
    *lock_owner_route() = Some(OwnerRoute { generation, sender });
    InteractiveTerminalOwner {
        generation,
        receiver,
    }
}

#[derive(Debug, Error)]
pub enum InteractiveTerminalAcquireError {
    #[error(
        "interactive hooks require the embedded Codex TUI; no local terminal owner is registered"
    )]
    NoLocalOwner,
    #[error("the embedded Codex TUI disconnected before accepting the terminal lease")]
    OwnerDisconnected,
    #[error("the embedded Codex TUI rejected the terminal lease: {0}")]
    Rejected(String),
}

/// A granted terminal lease. Dropping it wakes the TUI so it can restore Codex rendering.
pub struct InteractiveTerminalLease {
    finished: Option<oneshot::Sender<()>>,
    _permit: OwnedSemaphorePermit,
}

impl Drop for InteractiveTerminalLease {
    fn drop(&mut self) {
        if let Some(finished) = self.finished.take() {
            let _ = finished.send(());
        }
    }
}

/// Wait until the embedded TUI has actually relinquished its terminal before returning.
///
/// The global permit ensures multiple matching interactive handlers cannot contend for `/dev/tty`.
pub async fn acquire() -> Result<InteractiveTerminalLease, InteractiveTerminalAcquireError> {
    let permit = Arc::clone(lease_semaphore())
        .acquire_owned()
        .await
        .map_err(|_| InteractiveTerminalAcquireError::OwnerDisconnected)?;
    let sender = lock_owner_route()
        .as_ref()
        .map(|route| route.sender.clone())
        .ok_or(InteractiveTerminalAcquireError::NoLocalOwner)?;

    let (ready_tx, ready_rx) = oneshot::channel();
    let (finished_tx, finished_rx) = oneshot::channel();
    sender
        .send(InteractiveTerminalRequest {
            lease_id: Uuid::new_v4().to_string(),
            ready: ready_tx,
            finished: finished_rx,
        })
        .map_err(|_| InteractiveTerminalAcquireError::OwnerDisconnected)?;

    match ready_rx.await {
        Ok(Ok(())) => Ok(InteractiveTerminalLease {
            finished: Some(finished_tx),
            _permit: permit,
        }),
        Ok(Err(error)) => Err(InteractiveTerminalAcquireError::Rejected(error)),
        Err(_) => Err(InteractiveTerminalAcquireError::OwnerDisconnected),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    #[serial_test::serial(interactive_terminal)]
    async fn owner_acknowledges_and_serializes_terminal_leases() {
        assert!(matches!(
            acquire().await,
            Err(InteractiveTerminalAcquireError::NoLocalOwner)
        ));

        let mut owner = register_owner();
        let first_acquire = tokio::spawn(acquire());
        let first_request = owner.recv().await.expect("first lease request");
        let first_finished = first_request.finished;
        first_request.ready.send(Ok(())).expect("acknowledge first");
        let first_lease = first_acquire
            .await
            .expect("first acquire task")
            .expect("first lease");

        let second_acquire = tokio::spawn(acquire());
        assert!(
            tokio::time::timeout(Duration::from_millis(25), owner.recv())
                .await
                .is_err(),
            "second request must wait for the first lease"
        );

        drop(first_lease);
        first_finished.await.expect("first completion signal");

        let second_request = owner.recv().await.expect("second lease request");
        let second_finished = second_request.finished;
        second_request
            .ready
            .send(Ok(()))
            .expect("acknowledge second");
        let second_lease = second_acquire
            .await
            .expect("second acquire task")
            .expect("second lease");
        drop(second_lease);
        second_finished.await.expect("second completion signal");

        drop(owner);
        assert!(matches!(
            acquire().await,
            Err(InteractiveTerminalAcquireError::NoLocalOwner)
        ));
    }
}
