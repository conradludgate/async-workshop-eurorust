# Task 0 - Creating our workspace

To set ourselves up for the remainder of the workshop, let's create a new cargo workspace.

### Clone the template
TODO: insert link to workspace template for the workshop here

### Manually create the workspace

Create a new directory, and inside it create a file called `Cargo.toml`, with the contents

```toml
[workspace]
resolver = "2"
members = ["projects/*"]

[workspace.package]
edition = "2021"
```

Then, create another file in `projects/00_tokio/Cargo.toml` with the contents

```toml
[package]
name = "project_00_tokio"
version = "0.0.1"
edition.workspace = true

[dependencies]
```

We will later create some more projects for the upcoming chapters inside the same workspace.
