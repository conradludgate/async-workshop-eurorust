use std::{
    collections::VecDeque,
    future::poll_fn,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    time::Duration,
};

use tokio::time::Instant;

struct Channel<T> {
    data: VecDeque<T>,
    /// The single consumer waker, if any
    recv: Option<Waker>,
    /// is the receiver still there?
    receiver: bool,
    /// how many senders are still there?
    senders: usize,
}

pub struct Receiver<T> {
    channel: Arc<Mutex<Channel<T>>>,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| {
            let mut channel = self.channel.lock().unwrap();
            if let Some(t) = channel.data.pop_front() {
                return Poll::Ready(Some(t));
            }

            if channel.senders == 0 {
                return Poll::Ready(None);
            }

            // set our waker and return pending to pause
            // we will resume when the waker is used.
            channel.recv = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }
}

// Try and wake the receiver if we are the last sender to drop.
// This ensures the sender will known when to exit
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut channel = self.channel.lock().unwrap();
        channel.receiver = false;
    }
}

pub struct Sender<T> {
    channel: Arc<Mutex<Channel<T>>>,
}

impl<T: Send> Sender<T> {
    /// Send a message over the channel.
    ///
    /// # Errors
    ///
    /// Errors if the channel is closed.
    pub fn send(&self, t: T) -> Result<(), T> {
        let mut channel = self.channel.lock().unwrap();
        if !channel.receiver {
            return Err(t);
        }

        channel.data.push_back(t);

        if let Some(waker) = channel.recv.take() {
            waker.wake();
        }

        Ok(())
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let mut channel = self.channel.lock().unwrap();
        channel.senders += 1;
        Self {
            channel: Arc::clone(&self.channel),
        }
    }
}

// Try and wake the receiver if we are the last sender to drop.
// This ensures the sender will known when to exit
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut channel = self.channel.lock().unwrap();
        channel.senders -= 1;
        if channel.senders == 0 {
            if let Some(waker) = channel.recv.take() {
                waker.wake();
            }
        }
    }
}

/// Creates an unbounded mpsc channel for communicating between asynchronous
/// tasks without backpressure.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Mutex::new(Channel {
        data: VecDeque::new(),
        recv: None,
        receiver: true,
        senders: 1,
    }));

    let tx = Sender {
        channel: Arc::clone(&channel),
    };
    let rx = Receiver { channel };
    (tx, rx)
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = channel();

    let tx1 = tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        tx1.send(1).expect("channel should be open");
    });

    let tx2 = tx;
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        tx2.send(2).expect("channel should be open");
    });

    let now = Instant::now();
    while let Some(x) = rx.recv().await {
        println!("Received msg {x:?} after {dur:?}", dur = now.elapsed());
    }
    println!("Shutting down after {dur:?}", dur = now.elapsed());
}
