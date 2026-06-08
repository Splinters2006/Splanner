# Splanner

A touch-friendly family weekly planner for a living-room tablet.

The app runs as a small local Rust web server and stores planner data in plain
text files under `data/`. Everyone signs in with their account name and 4 digit
PIN, can add tasks, events, and broadcasts, and can assign items to one or more
people or groups.

## Features

- Weekly planner view with a shared 00:00-24:00 timeline.
- Events appear in the left timeline lane and stretch to match their duration.
- Timed tasks appear in the right timeline lane for the hour before their due
  time. Untimed tasks appear below the timeline.
- Tasks and events can be created for any date, including dates outside the
  currently visible week.
- The task filter can show everyone, any person, or any group for every user.
- People and groups are visually separated wherever they are selectable.
- Notes open in a centered popup instead of expanding inside cards.
- Cards show a blue border when the signed-in user is a recipient, and a green
  border when the signed-in user created the item.
- Broadcasts can be sent to people, groups, or everyone.

## Development

```sh
./Splanner.sh setup
./Splanner.sh update
cargo check
cargo test
cargo run
```

Open <http://127.0.0.1:8100/> after `cargo run`.

The Rust server code is in `src/main.rs`. The frontend is split into:

- `src/index.html`
- `src/styles.css`
- `src/app.js`

These files are embedded into the Rust binary with `include_str!`, so rebuild or
rerun the server after changing frontend assets.

`./Splanner.sh setup` updates from the git repository, installs Rust/Cargo when
missing, builds the app, optionally requests UPnP router port forwarding,
optionally configures Tailscale remote access, asks for the admin password used
by the browser UI to add or delete accounts, and installs Splanner as a systemd
service.

Accounts are created from the admin cogwheel. Each account needs a name and
4 digit PIN. There are no default accounts.

Groups are also managed from the admin cogwheel. Admins can create groups and
add or remove people from them. Groups appear separately from people when
assigning tasks, events, or broadcasts, and when filtering the planner.

Splanner has a built-in undeletable `overview` account for living-room tablet
display mode. Log in as `overview` with the admin password. It shows only the
current week and hides navigation, admin, delete, and task-entry controls.

`./Splanner.sh update` pulls the latest git changes, rebuilds the release
binary, and restarts the systemd service without repeating the admin password or
UPnP setup prompts.

If UPnP is enabled, setup installs the `upnpc` client when possible, switches
the app to listen on `0.0.0.0:8100`, and asks the router to forward TCP port
`8100` to this device. If UPnP is disabled, the app stays local-only on
`127.0.0.1:8100`.

If Tailscale is enabled, setup installs Tailscale when needed, starts
`tailscaled`, runs `sudo tailscale up`, switches the app to listen on
`0.0.0.0:8100`, and prints a Tailscale URL such as
`http://100.x.y.z:8100/`. Other devices must be on the same Tailnet to use it.

Rerun `./Splanner.sh setup` later only when you want to redo setup choices or
reset the admin password.

After setup, Splanner starts automatically on boot. Useful service commands:

```sh
sudo systemctl status splanner.service
sudo systemctl restart splanner.service
sudo journalctl -u splanner.service
```
