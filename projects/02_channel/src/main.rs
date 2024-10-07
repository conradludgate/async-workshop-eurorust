use std::{
    collections::VecDeque,
    future::poll_fn,
    sync::{Arc, Mutex, Weak},
    task::{Poll, Waker},
    time::Duration,
};

use tokio::time::Instant;

struct Channel<T> {
    data: VecDeque<T>,
    // The single consumer waker, if any
    recv: Option<Waker>,
}

pub struct Receiver<T> {
    channel: Arc<Mutex<Channel<T>>>,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| {
            match Arc::get_mut(&mut self.channel) {
                // all receivers have hung up, so we have exclusive access
                Some(channel) => Poll::Ready(channel.get_mut().unwrap().data.pop_front()),
                None => {
                    let mut channel = self.channel.lock().unwrap();
                    if let Some(t) = channel.data.pop_front() {
                        return Poll::Ready(Some(t));
                    }

                    // set our waker and return pending to pause
                    // we will resume when the waker is used.
                    channel.recv = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await
    }
}

#[derive(Clone)]
pub struct Sender<T> {
    channel: Weak<Mutex<Channel<T>>>,
    sender: Arc<()>,
}

impl<T: Send> Sender<T> {
    /// Send a message over the channel.
    ///
    /// # Errors
    ///
    /// Errors if the channel is closed.
    pub fn send(&self, t: T) -> Result<(), T> {
        let Some(channel) = self.channel.upgrade() else {
            return Err(t);
        };
        let mut channel = channel.lock().unwrap();
        channel.data.push_back(t);

        if let Some(waker) = channel.recv.take() {
            waker.wake();
        }

        Ok(())
    }
}

// Try and wake the receiver if we are the last sender to drop.
// This ensures the sender will known when to exit
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let Some(_last_sender) = Arc::get_mut(&mut self.sender) else {
            return;
        };
        let Some(channel) = self.channel.upgrade() else {
            return;
        };
        let mut channel = channel.lock().unwrap();
        if let Some(waker) = channel.recv.take() {
            waker.wake();
        }
    }
}

/// Creates an unbounded mpsc channel for communicating between asynchronous
/// tasks without backpressure.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Mutex::new(Channel {
        data: VecDeque::new(),
        recv: None,
    }));
    let sender = Arc::new(());

    let tx = Sender {
        channel: Arc::downgrade(&channel),
        sender,
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
