# Splanner

A touch-friendly family weekly planner for a living-room tablet.

The app runs as a small local Rust web server and stores planner items in the
browser's local storage. Everyone can add tasks, assign them to someone, and
record who asked for the task.

## Development

```sh
./setup.sh
cargo check
cargo test
cargo run
```

Open <http://127.0.0.1:8100/> after `cargo run`.

The setup script updates from the git repository, installs Rust/Cargo when
missing, builds the app, and asks for the admin password used by the browser UI
to add or delete accounts.

Rerun `./setup.sh` later to pull updates, rebuild, and reset the admin password.
