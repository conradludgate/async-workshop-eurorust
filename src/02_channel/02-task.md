# Task 1 - Writing our own mpsc

Let's write a very in-efficient mpsc channel.

Since we want a queue of data, let's model it with a `VecDeque`. Since we want this queue to be shared across tasks and threads,
we will want to make use of `Arc` and `Mutex`.

```rust
struct Channel<T> {
    queue: VecDeque<T>,
}

struct Receiver<T> {
    inner: Arc<Mutex<Channel<T>>
}

struct Sender<T> {
    inner: Arc<Mutex<Channel<T>>
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
