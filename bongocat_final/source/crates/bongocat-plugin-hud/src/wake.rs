//! Waking bounded channels for the service's event sources.
use std::{
    sync::{Arc, Mutex, mpsc},
    thread::Thread,
    time::Duration,
};

#[derive(Clone, Default)]
pub struct WorkerWake(Arc<Mutex<Option<Thread>>>);
impl WorkerWake {
    pub fn bind(&self, thread: Thread) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(thread);
    }
    pub fn wake(&self) {
        if let Some(thread) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            thread.unpark();
        }
    }
}
pub struct WakingSender<T> {
    sender: mpsc::SyncSender<T>,
    wake: WorkerWake,
}
impl<T> Clone for WakingSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            wake: self.wake.clone(),
        }
    }
}
impl<T> WakingSender<T> {
    pub fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let result = self.sender.send(value);
        self.wake.wake();
        result
    }
    pub fn try_send(&self, value: T) -> Result<(), mpsc::TrySendError<T>> {
        let result = self.sender.try_send(value);
        self.wake.wake();
        result
    }
}
pub struct WakingReceiver<T> {
    receiver: mpsc::Receiver<T>,
    wake: WorkerWake,
}
impl<T> WakingReceiver<T> {
    pub fn set_waker(&self, thread: Thread) {
        self.wake.bind(thread);
    }
    pub fn try_recv(&self) -> Result<T, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}
pub fn waking_channel<T>(capacity: usize) -> (WakingSender<T>, WakingReceiver<T>) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let wake = WorkerWake::default();
    (
        WakingSender {
            sender,
            wake: wake.clone(),
        },
        WakingReceiver { receiver, wake },
    )
}
