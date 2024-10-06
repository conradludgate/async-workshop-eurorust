# Tutorial - State Machines

Let's again use the following async function as a reference point:

```rust
async fn greet(name: String) {
    println!("hello {name}");
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("goodbye {name}");
}
```

and let's just remind ourselves of what a state machine looks like (we shall ignore `Pin` for now)

```rust
trait Future {
    type Output;

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output>;
}
```

Before we try to figure out what states we need to keep, let's consider the transitions instead.

The first transition is

```rust
println!("hello {name}");
tokio::time::sleep(Duration::from_secs(1))
```

The second transition is

```rust
println!("goodbye {name}");
```

Since we can pause at `.await` points, these have to be the places we save the states.
Then between the `.await`s, including before the first and after the last, are our transition steps.

Since our async fn has only 1 await point, that will be our one intermediate state.

We will also have 2 more states to complement our transitions. One state for before the function runs,
and one state for after the function is complete.

---

So let's see some code showing this off the states

```rust
enum GreetFut {
    Init {
        // Our initial state stores the argument of the function
        name: String,
    }
    Intermediate {
        // We must carry all values that are still in scope.
        name: String,

        // We must carry any intermediate futures that are still in progress
        sleep_fut: Sleep,
    }
    // When the future is done, there's no more values to store
    // as they have been dropped already.
    Done,
}
```

And now the transitions:

```rust
impl Future for GreetFut {
    // our function returns nothing
    type Output = ();

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match *self {
                GreetFut::Init { name } => {
                    // perform our first transition
                    println!("hello {name}");
                    let sleep_fut = tokio::time::sleep(Duration::from_secs(1));

                    // update our state
                    *self = GreetFut::Intermediate { name, sleep_fut };

                    // we need to try and continue the state machine
                    continue;
                }
                GreetFut::Intermediate { name, mut sleep_fut } => {
                    // check if we are ready to make the second transition
                    ready!(sleep_fut.poll(cx));

                    // we are ready to perform our second transition
                    println!("goodbye {name}");

                    // update our state again
                    *self = GreetFut::Done;

                    // since there are no more state, we should return ready
                    return Poll::Ready(());
                }
                // There is no state after done, so we cannot make any transition.
                GreetFut::Done => panic!("polled after completion"),
            }
        }
    }
}
```
