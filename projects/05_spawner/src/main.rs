use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, Weak},
    task::{Context, Poll, Wake, Waker},
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
    /// the state the worker is currently in
    state: WorkerState,
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

                // park until we are notified to be ready
                while worker.state != WorkerState::Ready {
                    worker.state = WorkerState::Parked;
                    worker = runtime.park.wait(worker).unwrap();
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

fn main() {
    block_on(async move {
        let (watch_tx, watch_rx) = tokio::sync::watch::channel(true);

        for i in 0..10 {
            let mut watch_rx = watch_rx.clone();
            spawn(async move {
                // wait until we are no longer running
                watch_rx.wait_for(|running| !*running).await.unwrap();
                // bad_sleep(start + std::time::Duration::from_secs(1)).await;
                println!("completed {i}")
            });
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            watch_tx.send(false).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));
            tx.send(()).unwrap();
        });

        rx.await.unwrap();
    });

    println!("done");
}
