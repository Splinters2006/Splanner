#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

APP_PORT=8100
SERVICE_NAME=splanner.service

usage() {
  cat <<EOF
Usage: ./Splanner.sh <command>

Commands:
  setup    First-time setup: update, install dependencies, build, configure UPnP,
           set admin password, install/restart systemd service.
  update   Update from git, build, and restart the systemd service without
           changing admin password or setup choices.
EOF
}

install_package() {
  package_name="$1"
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y "$package_name"
  elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y "$package_name"
  elif command -v pacman >/dev/null 2>&1; then
    sudo pacman -Sy --needed "$package_name"
  elif command -v brew >/dev/null 2>&1; then
    brew install "$package_name"
  else
    echo "Could not install $package_name automatically. Install it manually and rerun setup."
    return 1
  fi
}

ensure_upnpc() {
  if command -v upnpc >/dev/null 2>&1; then
    return 0
  fi

  echo "Installing UPnP client dependency..."
  install_package miniupnpc
}

ensure_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    return
  fi

  echo "Rust/Cargo is not installed. Installing with rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
}

update_from_git() {
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "Updating Splanner from git..."
    git pull --ff-only
  fi
}

build_release() {
  ensure_cargo
  echo "Building Splanner..."
  cargo build --release
}

detect_lan_ip() {
  if command -v hostname >/dev/null 2>&1; then
    hostname -I 2>/dev/null | awk '{print $1}' || true
  elif command -v ipconfig >/dev/null 2>&1; then
    ipconfig getifaddr en0 2>/dev/null || true
  fi
}

configure_upnp() {
  read -rp "Enable automatic UPnP router port forwarding for port $APP_PORT? [y/N] " enable_upnp
  case "$enable_upnp" in
    [yY]|[yY][eE][sS])
      if ! ensure_upnpc; then
        echo "Skipping UPnP because the UPnP client could not be installed."
        printf '127.0.0.1:%s\n' "$APP_PORT" > data/bind_address.txt
        printf 'failed\n' > data/upnp.txt
        return
      fi
      lan_ip="$(detect_lan_ip)"
      if [[ -z "$lan_ip" ]]; then
        read -rp "Could not detect LAN IP. Enter this device's LAN IP: " lan_ip
      fi
      if [[ -z "$lan_ip" ]]; then
        echo "Skipping UPnP because no LAN IP was provided."
        printf '127.0.0.1:%s\n' "$APP_PORT" > data/bind_address.txt
        return
      fi

      printf '0.0.0.0:%s\n' "$APP_PORT" > data/bind_address.txt
      echo "Requesting router port mapping TCP $APP_PORT -> $lan_ip:$APP_PORT..."
      if upnpc -e Splanner -a "$lan_ip" "$APP_PORT" "$APP_PORT" TCP; then
        printf 'enabled\n' > data/upnp.txt
        echo "UPnP mapping requested. Router support and firewall rules may still affect access."
      else
        printf 'failed\n' > data/upnp.txt
        echo "UPnP mapping failed. Splanner will still listen on the LAN at $lan_ip:$APP_PORT."
      fi
      ;;
    *)
      printf '127.0.0.1:%s\n' "$APP_PORT" > data/bind_address.txt
      printf 'disabled\n' > data/upnp.txt
      ;;
  esac
}

install_systemd_service() {
  if ! command -v systemctl >/dev/null 2>&1; then
    echo "systemd is not available on this machine; skipping service setup."
    return
  fi

  repo_dir="$(pwd)"
  binary_path="$repo_dir/target/release/splanner"
  service_user="$(id -un)"
  service_group="$(id -gn)"

  echo "Installing systemd service..."
  service_file="$(mktemp)"
  cat > "$service_file" <<EOF
[Unit]
Description=Splanner family planner
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$service_user
Group=$service_group
WorkingDirectory=$repo_dir
ExecStart=$binary_path
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

  sudo install -m 0644 "$service_file" "/etc/systemd/system/$SERVICE_NAME"
  rm -f "$service_file"
  sudo systemctl daemon-reload
  sudo systemctl enable "$SERVICE_NAME"
  sudo systemctl restart "$SERVICE_NAME"
}

restart_systemd_service() {
  if ! command -v systemctl >/dev/null 2>&1; then
    echo "systemd is not available on this machine; skipping service restart."
    return
  fi

  if systemctl list-unit-files "$SERVICE_NAME" >/dev/null 2>&1; then
    echo "Restarting systemd service..."
    sudo systemctl restart "$SERVICE_NAME"
  else
    echo "Systemd service is not installed yet. Run ./Splanner.sh setup first."
  fi
}

set_admin_password() {
  while true; do
    read -rsp "Choose admin password: " admin_password
    echo
    read -rsp "Confirm admin password: " admin_password_confirm
    echo

    if [[ -z "$admin_password" ]]; then
      echo "Password cannot be empty."
    elif [[ "$admin_password" != "$admin_password_confirm" ]]; then
      echo "Passwords do not match."
    else
      break
    fi
  done

  printf '%s' "$admin_password" | target/release/splanner --set-admin-password
  unset admin_password admin_password_confirm
}

print_access_info() {
  echo "Open locally: http://127.0.0.1:$APP_PORT/"
  if [[ "$(cat data/bind_address.txt 2>/dev/null)" == "0.0.0.0:$APP_PORT" ]]; then
    lan_ip="$(detect_lan_ip)"
    if [[ -n "$lan_ip" ]]; then
      echo "Open on your LAN: http://$lan_ip:$APP_PORT/"
    fi
  fi
}

run_setup() {
  update_from_git
  build_release
  mkdir -p data
  configure_upnp
  set_admin_password
  install_systemd_service

  echo
  echo "Setup complete."
  echo "Splanner service: sudo systemctl status $SERVICE_NAME"
  print_access_info
}

run_update() {
  update_from_git
  build_release
  restart_systemd_service

  echo
  echo "Update complete."
  echo "Splanner service: sudo systemctl status $SERVICE_NAME"
  print_access_info
}

command="${1:-}"
case "$command" in
  setup)
    run_setup
    ;;
  update)
    run_update
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage
    exit 2
    ;;
esac
