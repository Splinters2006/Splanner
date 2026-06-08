#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
exec ./Splanner.sh setup "$@"
