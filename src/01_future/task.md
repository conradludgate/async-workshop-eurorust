# Task 1 - Writing a select future

Now that we have a basic understanding of some state machine ideas, and how to use `Pin`,
let's construct an async function that races two futures.

In your project template you will see the following code

```rust
use std::{future::Future, time::Duration};

#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

async fn select<A: Future, B: Future>(left: A, right: B) -> Either<A::Output, B::Output> {
    // REPLACE ME
    tokio::select! {
        left = left => Either::Left(left),
        right = right => Either::Right(right),
    }
}

#[tokio::main]
async fn main() {
    let (tx, rx) = tokio::sync::oneshot::channel();

    tokio::task::spawn(async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let _ = tx.send(());
    });

    let left = tokio::time::sleep(Duration::from_secs(3));
    let right = rx;

    let res = select(left, right).await;

    println!("raced: {:?}", res);
}
```

When run, we should expect to see the output

> raced: Right(Ok(()))

If you adjust the sleep durations, we might see a different result.

I want you to write your own implementation of what `select!()` is doing here.

---

For some insight here, let's take a simple async function that just calls another async function `F`

```rust
async fn run_one<F: Future>(f: F) -> F::Output {
    f.await
}
```

We know how to write this with 3 states, but we can cheat here and try inlining.

```rust
fn run_one<F: Future>(f: F) -> RunOneFut<F> {
    RunOneFut { f }
}

pin_project!{
    struct RunOneFut<F> {
        #[pin]
        f: F,
    }
}

impl<F: Future> Future for RunOneFut<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let output = ready!(this.f.poll(cx));

        Poll::Ready(output)
    }
}
```

There's no need to add our own state machine on top when we know F will be managing the states for us.

Now, the key insight here is what ready! is doing. Let's expand it out.

```rust
impl<F: Future> Future for RunOneFut<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let output = match this.f.poll(cx) {
            Poll::Ready(output) => output,
            Poll::Pending => return Poll::Pending,
        }

        Poll::Ready(output)
    }
}
```

Let's flip the script, so to speak, and return the ready result early and only return pending afterwards.

```rust
impl<F: Future> Future for RunOneFut<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.f.poll(cx) {
            Poll::Ready(output) => return Poll::Ready(output),
            Poll::Pending => {},
        }

        Poll::Pending
    }
}
```

In theory, there's nothing stopping you from doing something else before returning pending...
