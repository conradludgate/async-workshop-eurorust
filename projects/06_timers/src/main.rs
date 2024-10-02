use std::{
    cell::RefCell,
    collections::{binary_heap::PeekMut, BinaryHeap, VecDeque},
    future::{poll_fn, Future},
    pin::Pin,
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

struct Task {
    fut: Mutex<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
}

struct Runtime {
    park: Condvar,
    worker: Mutex<Worker>,
}

struct Worker {
    /// whether the root task is ready
    root_task: bool,
    /// all the spawned tasks that are ready
    tasks: VecDeque<Arc<Task>>,
    /// the timers in a min-heap
    timers: BinaryHeap<TimerEntry>,
    /// the state the worker is currently in
    state: WorkerState,
}

impl Worker {
    fn update_timers<'rt>(
        mut worker: MutexGuard<'rt, Self>,
        runtime: &'rt Runtime,
    ) -> (MutexGuard<'rt, Self>, Option<Duration>) {
        loop {
            let Some(next_timer) = worker.timers.peek_mut() else {
                break (worker, None);
            };

            let now = Instant::now();
            if next_timer.deadline > now {
                let timeout = next_timer.deadline - now;
                drop(next_timer);
                break (worker, Some(timeout));
            }

            let waker = PeekMut::pop(next_timer).waker;

            // we must drop the worker lock as the waker will probably try to acquire the lock
            // we don't want to deadlock!!
            drop(worker);
            waker.wake();
            worker = runtime.worker.lock().unwrap();
        }
    }
}

struct TimerEntry {
    deadline: Instant,
    waker: Waker,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}

impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap works as a max-heap by default.
        // We want a min-heap based on deadline.
        self.deadline.cmp(&other.deadline).reverse()
    }
}

#[derive(PartialEq)]
enum WorkerState {
    Running,
    Parked,
    Ready,
}

struct SimpleWaker {
    runtime: Weak<Runtime>,
    /// The task this waker is related to.
    /// None if this is the root task.
    task: Option<Arc<Task>>,
}

impl Wake for SimpleWaker {
    fn wake(self: Arc<Self>) {
        let Some(runtime) = self.runtime.upgrade() else {
            // the runtime exited and is no longer running.
            return;
        };

        let mut worker = runtime.worker.lock().unwrap();

        if let Some(task) = &self.task {
            worker.tasks.push_back(task.clone());
        } else {
            worker.root_task = true;
        }

        // if the worker thread is parked, tell it to wake up.
        if worker.state == WorkerState::Parked {
            runtime.park.notify_one();
        }

        worker.state = WorkerState::Ready
    }
}

thread_local! {
    static RUNTIME: RefCell<Option<Arc<Runtime>>> = const { RefCell::new(None) };
}

pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);

    let runtime = Arc::new(Runtime {
        park: Condvar::new(),
        worker: Mutex::new(Worker {
            root_task: false,
            tasks: VecDeque::new(),
            timers: BinaryHeap::new(),
            state: WorkerState::Running,
        }),
    });

    let prev = RUNTIME.replace(Some(Arc::clone(&runtime)));

    let root_waker_state = Arc::new(SimpleWaker {
        runtime: Arc::downgrade(&runtime),
        task: None,
    });

    let root_waker = Waker::from(root_waker_state);

    let res = loop {
        match f.as_mut().poll(&mut Context::from_waker(&root_waker)) {
            Poll::Ready(output) => break output,
            Poll::Pending => {
                let mut worker = runtime.worker.lock().unwrap();

                while let Some(task) = worker.tasks.pop_front() {
                    drop(worker);

                    let task_waker_state = Arc::new(SimpleWaker {
                        runtime: Arc::downgrade(&runtime),
                        task: Some(task.clone()),
                    });

                    let task_waker = Waker::from(task_waker_state);

                    let mut f = task.fut.lock().unwrap();
                    _ = f.as_mut().poll(&mut Context::from_waker(&task_waker));
                    drop(f);

                    worker = runtime.worker.lock().unwrap();
                }

                let mut maybe_timeout;
                (worker, maybe_timeout) = Worker::update_timers(worker, &runtime);

                // park until we are notified to be ready
                // or until a timer is ready to trigger
                while worker.state != WorkerState::Ready {
                    worker.state = WorkerState::Parked;
                    if let Some(timeout) = maybe_timeout {
                        let timeout_result;
                        (worker, timeout_result) =
                            runtime.park.wait_timeout(worker, timeout).unwrap();

                        // if we timed out, update the timers again
                        if timeout_result.timed_out() {
                            (worker, maybe_timeout) = Worker::update_timers(worker, &runtime);
                        }
                    } else {
                        worker = runtime.park.wait(worker).unwrap()
                    }
                }

                // announce that we are running the task and are not idle.
                worker.state = WorkerState::Running;
            }
        }
    };

    RUNTIME.set(prev);

    res
}

pub fn spawn<F: Future<Output = ()> + Send + 'static>(f: F) {
    RUNTIME.with_borrow(|rt| {
        let runtime = rt.as_ref().expect("runtime should be set");
        let mut worker = runtime.worker.lock().unwrap();

        worker.tasks.push_back(Arc::new(Task {
            fut: Mutex::new(Box::pin(f)),
        }));

        // if the worker thread is parked, tell it to wake up.
        if worker.state == WorkerState::Parked {
            runtime.park.notify_one();
        }

        worker.state = WorkerState::Ready
    });
}

/// Sleep until the deadline, returning the time we actually woke up.
pub async fn sleep_until(deadline: Instant) -> Instant {
    let mut registered = false;
    poll_fn(|cx| {
        // if the deadline has elapsed, return.
        let now = Instant::now();
        if deadline <= now {
            return Poll::Ready(now);
        }
        if !registered {
            RUNTIME.with_borrow(|rt| {
                let runtime = rt.as_ref().expect("runtime should be set");
                let mut worker = runtime.worker.lock().unwrap();

                // insert the timer entry
                let waker = cx.waker().clone();
                worker.timers.push(TimerEntry { deadline, waker });
            });
            registered = true;
        }
        Poll::Pending
    })
    .await
}

fn main() {
    let start = std::time::Instant::now();
    let deadline = start + std::time::Duration::from_secs(1);
    let woken = block_on(async { sleep_until(deadline).await });

    let lag = woken - deadline;

    println!("{lag:?}");
}
