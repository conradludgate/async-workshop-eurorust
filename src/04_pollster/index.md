# Chapter 4 - Our first async runtime

As we saw in previous sections, our cooperative tasks boil down to a type that implements `Future` and
must be repeatedly `poll`ed in order to finish. We also have the `Context` and `Waker` system
that allows the runtime to sleep if there's currently nothing to do.

If we want our async program to run, then it needs to bridge between async poll-based functions,
and non-async blocking functions. Let's look at tokio for example:

```rust
#[tokio::main]
async fn main() {
    foo().await;
}
```

If you expand the `tokio::main`, macro, you will see:

```rust
fn main() {
    let body = async {
        foo().await;
    };

    #[allow(clippy::expect_used, clippy::diverging_sub_expression)]
    {
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed building the Runtime")
            .block_on(body);
    }
}
```

The key thing to look out for here is this `block_on` function. It takes the form of

```rust
/// Turns an async function into a blocking function
fn block_on<F: Future>(f: F) -> F::Output {
    todo!()
}
```

But how would we write this?

---

Defining the bare-minimum, we need to build something representing

```rust
/// Turns an async function into a blocking function
fn block_on<F: Future>(f: F) -> F::Output {
    // futures must be pinned to be polled!
    let mut f = std::pin::pin!(f);

    loop {
        let mut cx: Context = todo!();
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(r) => break r,
            Poll::Pending => continue,
        }
    }
}
```

But there's two outstanding problems here.

1. How do we build our `Context`?
2. How do we allow the thread to sleep while idle

---

Let's tackle the first problem. There's a bit of a chain we will need to take.

To construct a `Context`, we can provider a `&Waker` to the `Context::from_waker()` function.
So, how do we construct a `Waker`? Conveniently, there's a `impl<W: Wake> From<Arc<W>> for Waker`,
so assuming we have some `Arc<impl Wake>`, we can construct a `Waker` and thus a `Context`.

Let's see the `Wake` trait:

```rust
pub trait Wake {
    // Required method
    fn wake(self: Arc<Self>);
}
```

All we need to provide is a `wake` function. Convenient!

For now, let's define it as a no-op. We will take it in the next step.

```rust
struct SimpleWaker {}

impl Wake for SimpleWaker {
    fn wake(self: Arc<Self>) {}
}
```

Now, we need to construct the waker and context in our `block_on` function.

```rust
/// Turns an async function into a blocking function
fn block_on<F: Future>(f: F) -> F::Output {
    // futures must be pinned to be polled!
    let mut f = std::pin::pin!(f);

    let root_waker_state = Arc::new(SimpleWaker {});
    let root_waker = Waker::from(root_waker_state);

    loop {
        let mut cx = Context::from_waker(&root_waker);
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(r) => break r,
            Poll::Pending => continue,
        }
    }
}
```

And now our code should run. Try it!

The only problem is that it uses 100% CPU while it's running. Not great!
