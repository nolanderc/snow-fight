//! A synchronous one-shot channel (a single use 1-capacity channel).

use std::sync::mpsc;

pub type SendError<T> = mpsc::SendError<T>;
pub type RecvError = mpsc::RecvError;
pub type TryRecvError = mpsc::TryRecvError;

/// Create a channel with a capacity of a single value.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    (Sender { sender }, Receiver { receiver })
}

/// A channel that may only be used once.
pub struct Sender<T> {
    sender: mpsc::SyncSender<T>,
}

/// A channel that may only be used once.
pub struct Receiver<T> {
    receiver: mpsc::Receiver<T>,
}

impl<T> Sender<T> {
    /// Send a value accross the channel.
    pub fn send(self, value: T) -> Result<(), SendError<T>> {
        self.sender.send(value)
    }
}

impl<T> Receiver<T> {
    /// Block until the value becomes available on the channel.
    pub fn recv(self) -> Result<T, RecvError> {
        self.receiver.recv()
    }

    /// Attempt to get the value on the channel, or return early
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }
}

