# Task 1 - Writing our own mpsc

Let's write a very in-efficient mpsc channel.

Since we want a queue of data, let's model it with a `VecDeque`. Since we want this queue to be shared across tasks and threads,
we will want to make use of `Arc` and `Mutex`.

```rust
struct Channel<T> {
    queue: VecDeque<T>,
}

struct Receiver<T> {
    channel: Arc<Mutex<Channel<T>>
}

struct Sender<T> {
    channel: Arc<Mutex<Channel<T>>
}
```

When sending data to the channel, it is as simple as locking the mutex, and pushing to the back of the queue.

```rust
impl<T> Sender<T> {
    pub fn send(&self, value: T) {
        let mut channel = self.channel.lock().unwrap();
        channel.queue.push_back(value);
    }
}
```

Now, let's recall the edge-cases we discussed earlier. One of them was that it's possible there's no receiver anymore.

Let's handle that by keeping track of whether the receiver is still active, and return an error when trying to send.

```rust
struct Channel<T> {
    queue: VecDeque<T>,

    /// true if there is still a receiver
    receiver: bool,
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut channel = self.channel.lock().unwrap();
        channel.receiver = false;
    }
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), T> {
        let mut channel = self.channel.lock().unwrap();
        if !channel.receiver {
            return Err(value)
        }

        channel.queue.push_back(value);
        Ok(())
    }
}
```

---

Now, that covers the sending side, but how do we receive data? In the happy case, there will always be
data ready immediatly that we can read

```rust
impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        let mut channel = self.channel.lock().unwrap();
        channel.queue.pop_front()
    }
}
```

But we also want to make sure we can wait for a message if there are currently none. To do so,
we will need to utilise the `Waker`.

We will use `std::future::poll_fn` for convenience:

```rust
struct Channel<T> {
    queue: VecDeque<T>,

    /// true if there is still a receiver
    receiver: bool,

    /// which receiver task to notify
    recv_waker: Option<Waker>,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        std::future::poll_fn(|cx| {
            let mut channel = self.channel.lock().unwrap();

            // if we have any values available, immediately return ready
            if let Some(value) = channel.queue.pop_front() {
                return Poll::Ready(Some(value));
            }

            // if there are no values, register our waker and return pending
            channel.recv_waker = Some(cx.waker().clone());

            Poll::Pending
        }).await
    }
}
```

Now, we just need to make senders wake up the task, if there is one currently waiting.

```rust
impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), T> {
        let mut channel = self.channel.lock().unwrap();
        if !channel.receiver {
            return Err(value)
        }

        // wake up the receiver
        if let Some(waker) = channel.recv_waker.take() {
            waker.wake();
        }

        channel.queue.push_back(value);
        Ok(())
    }
}
```

---

Now, there's just the last edge-case to cover. We need to keep track of how many `Sender`s are still attached to the channel,
and action on it accordingly. Let's start by adding a count to the channel, and cancel the recv if the count is 0.

```rust
struct Channel<T> {
    queue: VecDeque<T>,

    /// true if there is still a receiver
    receiver: bool,

    /// number of senders we have
    senders: usize,

    /// which receiver task to notify
    recv_waker: Option<Waker>,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        std::future::poll_fn(|cx| {
            let mut channel = self.channel.lock().unwrap();

            // if we have any values available, immediately return ready
            if let Some(value) = channel.queue.pop_front() {
                return Poll::Ready(Some(value));
            }

            // return early, do not wait, if there are no more senders
            if channel.senders == 0 {
                return Poll::Ready(None);
            }

            // if there are no values, register our waker and return pending
            channel.recv_waker = Some(cx.waker().clone());

            Poll::Pending
        }).await
    }
}
```

We also need to implement the correct accounting for the sender count

```rust
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let mut channel = self.channel.lock().unwrap();
        channel.senders += 1;
        Self { channel: Arc::clone(&self.channel) }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut channel = self.channel.lock().unwrap();
        channel.senders -= 1;

        // wake up the receiver if there are no more senders
        if channel.senders == 0 {
            if let Some(waker) = channel.recv_waker.take() {
                waker.wake();
            }
        }
    }
}
```

---

And that's it. We just need a function to construct our channel handles,

```rust
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Mutex::new(Channel {
        queue: VecDeque::new(),
        recv_waker: None,
        receiver: true,
        senders: 1,
    }));

    let tx = Sender {
        channel: Arc::clone(&channel),
    };
    let rx = Receiver { channel };
    (tx, rx)
}
```
