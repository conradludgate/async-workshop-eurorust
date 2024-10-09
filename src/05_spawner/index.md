# Chapter 5 - Dynamic task allocation

Now that we can run one task in our little runtime. Now is the time to run multiple tasks.
I suggest copy-pasting what we already have into the next project file

Again taking inspiration from tokio, we might want to utilise a spawn function that accepts
a future and then lazily schedules the task into the runtime.

```rust
pub fn spawn<F>(f: F)
where
    F: Future<Output = ()> + Send + 'static
{
    todo!()
}
```

For simplicity, we will pretend that all futures return no values, This might seem like an annoying restriction,
but it's not too difficult to work around by inserting your own channel into the task, and it makes the following
code quite a bit simpler.

---

Since we will support _any_ future, we need to store a dynamic collection of futures, Let's start off by
defining a helper type-alias

```rust
type BoxFut = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
```

You'll see we're going to be utilising box-pinning here. Since we want dynamic futures, we need to use dynamic dispatch.
Dynamic dispatch is easiest when you use boxed objects, so it just makes sense to use box pinning too.

While we're at it, we know we will likely need a queue of ready tasks - when tasks wake up they need to be scheduled -
so let's add a queue to the worker with a stubbed task type. We don't yet know what the task might look like.

```rust
struct Task {}

struct Worker {
    /// all the spawned tasks that are ready
    tasks: VecDeque<Task>,

    state: WorkerState,
}
```

---

Next, let's think about our `Waker`. Ideally, the waker will be able to identify the task directly.
If our runtime had thousands of tasks, we don't want to wake all of them.
A solution we could use is to store the task in the waker so it can be placed into the queue on wake.
Let's adapt our `SimpleWaker` from before:

```rust
struct TaskWaker {
    // we now store a task to push into the runtime worker queue
    task: Task,
    runtime: Arc<Runtime>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        let mut worker = self.worker.lock().unwrap();

        if worker.state == WorkerState::Parked {
            // notify the main parked thread
            self.park.notify_one();
        }

        worker.tasks.push_back(self.task.clone());

        // announce there is a task ready.
        worker.state = WorkerState::Ready;
    }
}
```

---

Finally, our worker will need to process the queue of tasks. The task must store a `BoxFut` type.
And since we need to share the task between the wakers and the runtime, we know the task must store this
behind an `Arc`. And since polling the future requires mut access, it must therefore also be behind a mutex.

```rust
#[derive(Clone)]
struct Task {
    fut: Arc<Mutex<BoxFut>>,
}
```

Now it's your turn to piece this all together
