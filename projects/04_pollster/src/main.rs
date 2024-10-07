use std::{
    future::Future,
    sync::{Arc, Condvar, Mutex, Weak},
    task::{Context, Poll, Wake, Waker},
};

struct Runtime {
    park: Condvar,
    worker: Mutex<Worker>,
}

struct Worker {
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
}

impl Wake for SimpleWaker {
    fn wake(self: Arc<Self>) {
        let Some(runtime) = self.runtime.upgrade() else {
            // the runtime exited and is no longer running.
            return;
        };

        let mut worker = runtime.worker.lock().unwrap();

        // if the worker thread is parked, tell it to wake up.
        if worker.state == WorkerState::Parked {
            runtime.park.notify_one();
        }

        worker.state = WorkerState::Ready
    }
}

pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);

    let runtime = Arc::new(Runtime {
        park: Condvar::new(),
        worker: Mutex::new(Worker {
            state: WorkerState::Running,
        }),
    });

    let root_waker_state = Arc::new(SimpleWaker {
        runtime: Arc::downgrade(&runtime),
    });

    let root_waker = Waker::from(root_waker_state);

    let res = loop {
        match f.as_mut().poll(&mut Context::from_waker(&root_waker)) {
            Poll::Ready(output) => break output,
            Poll::Pending => {
                let mut worker = runtime.worker.lock().unwrap();

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

    res
}

fn main() {
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(2));
        tx.send("hello world").unwrap();
    });

    let value = block_on(async { rx.await.unwrap() });

    println!("{value}");
}
