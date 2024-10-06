# Tutorial - Pin

We will have to use `Pin` during this workshop, so let's explore the ways we can use pin.

## Constructing pinned values.

Often, you don't need to construct any pinned values. When you use `.await`, Rust will automatically pin
the values accordingly. There are exceptions though.

There are 4 main ways to construct a pinned value:

### Easy mode

```rust
let pinned = Box::pin(value);
```

Since the value on the heap has a stable address, we can use box to construct a self-referential safe value. This will always work
and you can keep the owned value and pass around the owned value as usual.

### Cheating mode

```rust
let pinned = Pin::new(&mut value);
```

Some values do not care about whether they are pinned or not. Such types are called `Unpin`. For example, a `String` is not self referential,
thus implements the `Unpin` trait. When a function requires such a pinned value but pinning is unnecessary,
usually in generics or traits, you can construct the `Pin` reference on demand with `Pin::new`.

### Zero cost abstration

```rust
let pinned = std::pin::pin!(value);
```

If you never need ownership over the value, and just want to poll it, you can quite often pin inplace using this special macro.
This avoids the cost of the allocation of Box.
This works by internally moving and shadowing the value such that you cannot access it again. This is often known as stack-pinning, as
opposed to the box-pinning we saw earlier.

### I know what I am doing

```rust
let pinned = unsafe { Pin::new_unchecked(&mut value) };
```

If you know that the value is actually pinned, but cannot prove it to the compiler, you can use
unsafe to construct a Pin manually.

We won't have to do this during today's workshop.

## Pin Projections

Another common technique is pin projecting, where you take a pinned value,
and can therefore assert that the contained values are also pinned.

```rust
struct Foo {
    bar: Bar,
    baz: Baz,
}

struct FooProjection<'a> {
    bar: Pin<&'a mut Bar>,
    baz: Pin<&'a mut Baz>,
}

fn project(foo: Pin<&mut Foo>) -> FooProjection<'_> {
    unsafe {
        let foo = Pin::into_inner_unchecked(foo);

        let bar = Pin::new_unchecked(&mut foo.bar);
        let baz = Pin::new_unchedked(&mut foo.baz);

        FooProjection { bar, baz }
    }
}
```

Because this is such a common pattern, there exist some tools that help you do it safely.

```rust
use pin_project_lite::pin_project;

pin_project! {
    struct Foo {
        #[pin]
        bar: Bar,
        #[pin]
        baz: Baz,
    }
}

fn demo(foo: Pin<&mut Foo>) {
    // the project function is provided for us
    let _foo = foo.project();
}
```

## Reborrowing

An unfortunate downside of Pin being a library type and not a built in reference type is that there's no automatic reborrowing.
This means you will commonly see `pinned_val.as_mut()` littered around the codebase to re-borrow the value with a temporary shorter lifetime.

```rust
fn demo(mut foo: Pin<&mut Foo>) {
    loop {
        // need to reborrow here so we don't move out of the loop.
        foo.as_mut().poll(some_context());
    }
}
```
