use std::{
    cell::UnsafeCell,
    collections::BTreeMap,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    time::Duration,
};

struct AsyncMutex<T> {
    // the state that manages tasks waiting to acquire the lock
    queue: Mutex<Queue>,
    // The data the mutex is protecting
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for AsyncMutex<T> {}
unsafe impl<T: Send> Sync for AsyncMutex<T> {}

pub struct AsyncMutexGuard<'a, T> {
    inner: &'a AsyncMutex<T>,
}

impl<T> Deref for AsyncMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner.data.get() }
    }
}
impl<T> DerefMut for AsyncMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inner.data.get() }
    }
}

impl<'a, T> Drop for AsyncMutexGuard<'a, T> {
    fn drop(&mut self) {
        let mut queue = self.inner.queue.lock().unwrap();
        assert!(!queue.unlocked);

        // wake the next task in the queue
        if let Some((_index, waker)) = queue.wait_queue.pop_first() {
            if let Some(waker) = waker {
                waker.wake();
            }
        } else {
            // no one in the queue, leave in an unlocked state.
            queue.unlocked = true;
        }
    }
}

struct Queue {
    // The current queue tail
    index: u64,
    // the queue of all tasks waiting to acquire the mutex
    wait_queue: BTreeMap<u64, Option<Waker>>,

    // the queue is currently unlocked.
    unlocked: bool,
}

impl<T> AsyncMutex<T> {
    pub const fn new(val: T) -> Self {
        Self {
            queue: Mutex::new(Queue {
                index: 0,
                wait_queue: BTreeMap::new(),
                unlocked: true,
            }),
            data: UnsafeCell::new(val),
        }
    }

    pub fn lock(&self) -> Acquire<T> {
        let mut queue = self.queue.lock().unwrap();
        let index = queue.index;
        queue.index += 1;

        if queue.unlocked {
            // if the lock is currently unlocked, mark it as unlocked so we can claim it when polling.
            assert!(queue.wait_queue.is_empty());
            queue.unlocked = false;
        } else {
            // register our interest to lock the mutex at the back of the queue
            queue.wait_queue.insert(index, None);
        }

        Acquire {
            mutex: self,
            index,
            acquired: false,
        }
    }
}

pub struct Acquire<'a, T> {
    mutex: &'a AsyncMutex<T>,
    index: u64,
    acquired: bool,
}

impl<'a, T> Future for Acquire<'a, T> {
    type Output = AsyncMutexGuard<'a, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let index = self.index;
        let mut queue = self.mutex.queue.lock().unwrap();
        assert!(!queue.unlocked);

        let Some(waker_slot) = queue.wait_queue.get_mut(&index) else {
            // if we were removed from the queue, that means we must be the next owner!
            self.acquired = true;
            return Poll::Ready(AsyncMutexGuard { inner: self.mutex });
        };

        // we are still waiting in the queue.
        *waker_slot = Some(cx.waker().clone());

        Poll::Pending
    }
}

impl<'a, T> Drop for Acquire<'a, T> {
    fn drop(&mut self) {
        // if we already acquired the lock, do nothing here.
        if self.acquired {
            return;
        }

        let index = self.index;
        let mut queue = self.mutex.queue.lock().unwrap();

        // we must remove ourselves from the wait queue if we are no longer waiting
        if queue.wait_queue.remove(&index).is_none() {
            // if we were removed from the queue already, that means we were about to be the next owner
            // we should notify the next in the queue

            // wake the next task in the queue
            if let Some((_index, waker)) = queue.wait_queue.pop_first() {
                if let Some(waker) = waker {
                    waker.wake();
                }
            } else {
                // no one in the queue, leave in an unlocked state.
                queue.unlocked = true;
            }
        };
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mutex = Arc::new(AsyncMutex::new(0));

    let mutex1 = mutex.clone();
    tokio::spawn(async move {
        println!("task 1 acquiring the lock");
        let mut lock = mutex1.lock().await;
        println!("task 1 acquired the lock");
        tokio::time::sleep(Duration::from_millis(2000)).await;
        *lock += 1;
        println!("task 1 releasing the lock");
    });

    let mutex2 = mutex.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        println!("task 2 acquiring the lock");
        let mut lock = mutex2.lock().await;
        println!("task 2 acquired the lock");
        tokio::time::sleep(Duration::from_millis(2000)).await;
        *lock += 1;
        println!("task 2 releasing the lock");
    });

    tokio::time::sleep(Duration::from_millis(1500)).await;
    println!("task 0 acquiring the lock");
    let val = *mutex.lock().await;
    println!("task 0 acquired the lock");

    println!("Shutting down with value {val}");
}
