# Chapter 3 - Writing an async mutex

A mutex is a lot like the mpsc channel we just looked at, except the inverse. Much like a channel, it's another
commonly reached tool when it comes to concurrent synchronisation, as you saw we used a mutex in the last project
as it makes mutating state between concurrent tasks trivial.

Unlike a channel, which has one waiter, and multiple concurrent writers. A mutex has multiple waiters and one non-concurrent writer.
We'll focus on fair mutexes, as that offers some strong latency guarantees, as opposed to faster un-fair mutexes. Because of this
fairness requirement, we will need a queue of waiting requests to lock the mutex.

![](./mutex-queue.png)

---

Because of the nature of a mutex, we will need some necessary unsafe in this project.
All of the unsafe code you will need is already provided in the project template, and
for the sake of the lesson I will not go into the details about why this is necessary.

---

Much like in the mpsc channel project, we will have some edge cases here too.

## Dropping the MutexGuard

If we drop a MutexGuard, we will want to notify the next task in the queue that
the mutex is now available. If there is no task currently waiting, we should
leave the mutex in a state that indicates it is immediately available.

## Dropping a waiting task

While our task is waiting to acquire access to the lock, we might cancel the task
before ever getting access.

I haven't mentioned async cancellation yet today,
but to since Future's are just data structs inbetween calls to poll,
they can be dropped at any time inbetween.

The issue with dropping the waiting task is that it could cause a deadlock.
Let's say we drop the waiting task but leave our slot in the waker queue.
When the mutex unlocks and tries to wake up the next task, nothing will happen!
There will never be another task that can unlock the mutex. So instead, when the
task to acquire the lock is dropped early, we should remove our entry from the queue.

One last edge case occurs here. We might drop the waiting task at the same time as we are notified
of our access to complete the acquisition of the lock. If this happens, we will need to forward along the notification
to the next in line.

---

Let's think about how we might implement this.

We obviously need a queue for our wakers to live, however we probably cannot use a VecDeque queue
like in our channel example. This is because of our need to remove the entries anywhere in the queue on demand.

Instea, we could use a `BTreeMap<u64, T>` to implement our queue. If we pair this with a monotonically increasing
counter, we can push to the end easily. Just increment the counter and use that as the key into the map.
Additionally, since the `BTreeMap` is ordered, we can use the provided `pop_first()` to take the next
waker in the queue when we want to wake one up.

Something like

```rust
struct AsyncMutex<T> {
    queue: Mutex<Queue>,
    data: UnsafeCell<T>,
}

struct Queue {
    /// none if we are just registering our position in the queue.
    /// some if we have registered the waker
    /// removed if we have access to acquire the lock
    wakers: BTreeMap<u64, Option<Waker>>,
    index: u64,
}
```

The rest just falls into place after this.

---

We need to notify a task when dropping out mutex guard

```rust
impl<'a, T> Drop for AsyncMutexGuard<'a, T> {
    fn drop(&mut self) {
        let mut queue = self.mutex.queue.lock().unwrap();

        // remove the next entry, signifying it's free to unlock the mutex
        if let Some(entry) = queue.wakers.pop_first() {
            // wake up the task if any
            if let Some(waker) = entry {
                waker.wake()
            }
        }
    }
}
```

Now we need to implement the acquire future

```rust
impl<T> AsyncMutex<T> {
    async fn lock(&self) -> AsyncMutexGuard<'_, T> {
        let mut queue = self.queue.lock().unwrap();

        // claim a key
        let key = queue.index;
        queue.index += 1;

        // register our interest with no waker
        queue.wakers.insert(key, None);

        drop(queue);

        Acquire { mutex: self, key }.await
    }
}

pub struct Acquire<'a, T> {
    mutex: &'a AsyncMutex<T>,
    key: u64,
}

impl<'a, T> Future for Acquire<'a, T> {
    type Output = AsyncMutexGuard<'a, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this: &mut Self = &mut *self;

        let mut queue = self.mutex.queue.lock().unwrap();

        // check if we are still in the queue
        let Some(entry) = queue.wakers.get_mut(self.index) else {
            // if we are not in the queue, we can acquire the lock!
            return Poll::Ready(AsyncMutexGuard { mutex: self.mutex });
        }

        // register our waker
        *entry = Some(cx.waker().clone());

        // return pending and wait for another notification
        Poll::Pending
    }
}
```

We also need to handle the acquire drop edge case

```rust
impl<'a, T> Drop for Acquire<'a, T> {
    fn drop(&mut self) {

        let mut queue = self.mutex.queue.lock().unwrap();

        // remove our registration
        if let Some(_entry) = queue.wakers.remove(self.index) {
            // nothing more to do
            return;
        }

        drop(queue);

        // since we weren't in the queue, we could have acquired the lock,
        // so we need to notify the next task.
        // let's do it simply by dropping the guard we would have acquired.

        let _ = AsyncMutexGuard { mutex: self.mutex };
    }
}
```

There's one more edge case to handle. The case where there are no more tasks waiting
for the lock. We can resolve this by adding an 'unlocked' boolean. If 'unlocked' is set,
we skip the queue and resolve the acquired mutex guard immediately, unsetting unlocked
at the same time.

```rust
struct Queue {
    /// none if we are just registering our position in the queue.
    /// some if we have registered the waker
    /// removed if we have access to acquire the lock
    wakers: BTreeMap<u64, Option<Waker>>,
    index: u64,

    /// true if the mutex is currently unlocked
    /// false means you must go to the waker queue
    unlocked: bool,
}

impl<'a, T> Drop for AsyncMutexGuard<'a, T> {
    fn drop(&mut self) {
        let mut queue = self.mutex.queue.lock().unwrap();

        // remove the next entry, signifying it's free to unlock the mutex
        if let Some(entry) = queue.wakers.pop_first() {
            // wake up the task if any
            if let Some(waker) = entry {
                waker.wake()
            }
        } else {
            // mark the mutex as unlocked
            queue.unlocked = true;
        }
    }
}

impl<T> AsyncMutex<T> {
    async fn lock(&self) -> AsyncMutexGuard<'_, T> {
        let mut queue = self.queue.lock().unwrap();

        // claim a key
        let key = queue.index;
        queue.index += 1;

        if queue.unlocked {
            // skip registering our interest, we can acquire the lock
            // but lets mark the mutex as locked again
            queue.unlocked = false;
        } else {
            // register our interest with no waker
            queue.wakers.insert(key, None);
        }

        drop(queue);

        Acquire { mutex: self, key }.await
    }
}
```

---

And that's it! A working asynchronous mutex. Rather than `Mutex::lock` blocking the thread,
it is able to cooperatively yield until it's actually ready to acquire.

Homework for the reader is to turn it into a more generalised semaphore. Semaphores allow
more than 1 task to acquire a work permit, but with a limit. In our case, we have implemented a semaphore
with a fixed limit of 1, but we could support more than 1.

Homework++, once you have turned this mutex into a semaphore,
could we use this to implement backpressure into our mpsc channel from before.
