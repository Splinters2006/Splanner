#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Updating Splanner from git..."
  git pull --ff-only
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust/Cargo is not installed. Installing with rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
fi

echo "Building Splanner..."
cargo build --release

mkdir -p data

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

echo
echo "Setup complete."
echo "Run Splanner with: cargo run"
echo "Open: http://127.0.0.1:8100/"
