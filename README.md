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
missing, builds the app, optionally requests UPnP router port forwarding, asks
for the admin password used by the browser UI to add or delete accounts, and
installs Splanner as a systemd service.

If UPnP is enabled, setup installs the `upnpc` client when possible, switches
the app to listen on `0.0.0.0:8100`, and asks the router to forward TCP port
`8100` to this device. If UPnP is disabled, the app stays local-only on
`127.0.0.1:8100`.

Rerun `./setup.sh` later to pull updates, rebuild, and reset the admin password.

After setup, Splanner starts automatically on boot. Useful service commands:

```sh
sudo systemctl status splanner.service
sudo systemctl restart splanner.service
sudo journalctl -u splanner.service
```
