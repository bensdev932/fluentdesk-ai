//! Deterministic session cancellation token (`T011`).

use tokio::sync::watch;

/// Value object encapsulating a cooperative cancellation signal for a speaking session.
#[derive(Clone, Debug)]
pub struct SessionCancellationToken {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl Default for SessionCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCancellationToken {
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }

    pub fn reset(&self) {
        let _ = self.sender.send(false);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    pub async fn wait_for_cancellation(&mut self) {
        while !*self.receiver.borrow_and_update() {
            if self.receiver.changed().await.is_err() {
                break;
            }
        }
    }
}
