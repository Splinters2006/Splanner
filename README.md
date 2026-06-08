# Splanner

A touch-friendly family weekly planner for a living-room tablet.

The app runs as a small local Rust web server and stores planner items in the
browser's local storage. Everyone signs in with their account name and 4 digit
PIN, can add tasks, assign tasks to one or more people, and records who asked
for the task.

## Development

```sh
./Splanner.sh setup
./Splanner.sh update
cargo check
cargo test
cargo run
```

Open <http://127.0.0.1:8100/> after `cargo run`.

`./Splanner.sh setup` updates from the git repository, installs Rust/Cargo when
missing, builds the app, optionally requests UPnP router port forwarding, asks
for the admin password used by the browser UI to add or delete accounts, and
installs Splanner as a systemd service.

Accounts are created from the admin cogwheel. Each account needs a name and
4 digit PIN. There are no default accounts.

`./Splanner.sh update` pulls the latest git changes, rebuilds the release
binary, and restarts the systemd service without repeating the admin password or
UPnP setup prompts.

If UPnP is enabled, setup installs the `upnpc` client when possible, switches
the app to listen on `0.0.0.0:8100`, and asks the router to forward TCP port
`8100` to this device. If UPnP is disabled, the app stays local-only on
`127.0.0.1:8100`.

Rerun `./Splanner.sh setup` later only when you want to redo setup choices or
reset the admin password.

After setup, Splanner starts automatically on boot. Useful service commands:

```sh
sudo systemctl status splanner.service
sudo systemctl restart splanner.service
sudo journalctl -u splanner.service
```
