# Chapter 0 - Getting started with Tokio

Tokio is an "asynchronous runtime" for Rust.

## Asynchronous execution

Asynchronous execution means the lack of synchronous execution.
Synchronous execution means that tasks are executed in a synchronised fashion.
This could mean that a synchronous task will wait for the previous task to complete before it continues.

Some tasks are necessarily synchronous, but not tasks need to be synchronised together. For example,
one thread can execute a synchronous set of tasks, but it is asynchronous with all the other threads by default.
We can then resynchonrise threads are specific points using features like **Mutexes** and **Channels**

In Rust, asynchronous execution often holds a stronger cultural meaning, which means it uses
the `async/.await` features of Rust. This is what we mean when we say that Tokio is an "asynchronous runtime".
Tokio depends directly on the `async` features built into the Rust language.

## Runtime

A runtime is a very abstract term. People often say that Rust has no runtime, which is or isn't true depending on
who you ask.

A runtime is just a piece of software that manages some components for you. For example:
* An Operating System is a runtime over the hardware
* A memory allocator is a runtime over the system memory pages
* libc is a runtime over unix-style operating systems

Rust has a very minimal runtime on top of the runtimes listed above. The Rust runtime manages
setting up the main entrypoint in an application, thread execution, as well as setting up panic routines.

So why do people say that Rust has no runtime? Compared to some other languages, Rust's runtime is a lot less involved.
After process or thread startup, Rust does not run any code that you didn't yourself write.

* Compared to an interpreted language, which has a runtime that parses each statement and executes it.
* Compared to a JIT compiled or Bytecode VM language, which dynamically compiles and recompiles instructions to a native optimised representation at runtime.
* Compared to a garbage-collected language, which regularly sweeps working memory to detect unused allocations and reclaim them.
* Compared to a language with green threads, which has its own thread scheduling system built in and regularly forces tasks to yield.

So, is this good or bad? For many uses of Rust, not having an intrusive runtime like a garbage collector or green-threads forced on your
projects can be a very important thing. Perhaps you are developing some soft-realtime system where latency is incredibly important,
you do not want the runtime slowing things down with a memory sweep or forced yield in the meantime.

However, not all applications have such strong requirements, thus you might opt for some convenience for getting good performance.
In the case of Tokio, it is a runtime with a cooperative green-thread scheduler with support for async-io. Cooperative scheduling means that
tasks are not forced to yield, but instead choose to yield at explicit points, but ultimately when the task does yield, it is up to Tokio
to choose what task to run next and when the previous task will get to run again.
