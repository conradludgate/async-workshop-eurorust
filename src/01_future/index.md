# Chapter 1 - Introducing the Future trait

If our asynchronous tasks are cooperative as we discussed before, how could we model that as a Rust trait?

We know that our task will run until some point, at which it will choose to pause and return. The current task state should be saved
so that it can be resumed later. We can then resume the task until it either pauses again, or completes and returns a value.

Since it has to update state, we know we will likely need a function that takes `&mut self`. Since the task will sometimes
return nothing as it pauses, and sometimes returns a value when it completes, we need an enum to track which state it is in

```rust
enum TaskProgress<T> {
    Waiting,
    Completed(T),
}
```

Maybe we can model our tasks with the following trait.

```rust
trait Task {
    type Output

    fn resume(&mut self) -> TaskProgress<Self::Output>;
}
```

---

One problem. We have no way to distinguish between pausing because we are being polite,
and pausing because we're blocked on some other task completing. We ultimately want
some system that allows a task to inform the runtime of its ability to resume.

Maybe we can introduce some notification system which allows tasks to supply such notifications.

```rust
struct ReadySignal(/* todo */);

impl ReadySignal {
    fn announce_readiness(&self);
}

trait Task {
    type Output

    fn resume(&mut self, ready: ReadySignal) -> TaskProgress<Self::Output>;
}
```

If a task is pausing for politeness, then it can announce its own readiness before returning `Waiting`.
If a task is blocked on some other task, then it can synchronise with the task, providing the dependency task
with its `ReadySignal`. When the task completes, it can check if any signals were registered against it, then
announce the readiness.

---

Unveiling our `Future`, we see that this construction is not too different from what we have in `std`.

```rust
enum Poll<T> {
    Pending,
    Ready(T),
}

struct Context<'a> { /* todo */ }

struct Waker { /* todo */ }

impl<'a> Context<'a> {
    fn waker(&self) -> &'a Waker;
}

trait Future {
    type Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}
```
