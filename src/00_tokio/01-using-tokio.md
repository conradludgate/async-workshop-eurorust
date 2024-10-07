# Task 1 - Using Tokio

In the `project/00_tokio` folder, we can see a simple async program. Let's go through step by step what we have here.

We shall start with the entrypoint. We must use the `#[tokio::main]` attribute if we want an async main function.

```rust
#[tokio::main]
async fn main() {

}
```

Let's next spawn some asynchronous tasks. Remember, these tasks execute synchronously relative to themselves, but are
unsynchronise relative to all other tasks. Inside the main function we can write

```rust
tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    println!("hello after 2 seconds");
})
```

If you run this program, you will notice that likely nothing gets printed. This is of course
because the task runs concurrently and is not being synchronised with the main function before our
process terminates.

We can resynchronise by introducing a channel.


```diff
 #[tokio::main]
 async fn main() {
+    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

     tokio::spawn(async move {
         tokio::time::sleep(std::time::Duration::from_secs(2)).await;
         println!("hello after 2 seconds");
+        tx.send(()).expect("channel should not be closed");
     })

+    rx.next().await.unwrap();
 }
```

For good measure, let's add another task and make our channels send some actual data.

```diff
 #[tokio::main]
 async fn main() {
+    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

     let tx1 = tx.clone();
     tokio::spawn(async move {
         tokio::time::sleep(std::time::Duration::from_secs(2)).await;
         println!("hello after 2 seconds");
         tx1.send(1).expect("channel should not be closed");
     })

+    let tx2 = tx.clone();
+    tokio::spawn(async move {
+        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
+        println!("hello after 1 second");
+        tx2.send(2).expect("channel should not be closed");
+    })

-    rx.next().await.unwrap();
+    let first = rx.next().await.unwrap();
+    let second = rx.next().await.unwrap();
+    println!("received {first} {second}");
 }
```

Our program should look like

```rust

```diff
#[tokio::main]
async fn main() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx1 = tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        println!("hello after 2 seconds");
        tx1.send(1).expect("channel should not be closed");
    })

    let tx2 = tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        println!("hello after 1 second");
        tx2.send(2).expect("channel should not be closed");
    })

    let first = rx.next().await.unwrap();
    let second = rx.next().await.unwrap();
    println!("received {first} {second}");
}
```

and when run it should eventually output

```
received 2 1
```
