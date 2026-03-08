#!/usr/bin/env bash
# Re-initialize Moneypenny: wipe data dir and run init again, then restore your config.
#
# Why delete the config? mp init refuses to run if moneypenny.toml already exists
# (to avoid overwriting a live project). So we backup the config, remove it, run
# init to create a fresh mp-data/ with the current schema, then put your config
# back. Your credentials and [sync] / [agents] settings are preserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CONFIG="moneypenny.toml"
ENABLE_HTTP=0
HTTP_PORT=8080
START_AFTER=0

usage() {
  cat <<'EOF'
Usage: scripts/reinit.sh [options]

Options:
  --config <file>   Config file relative to repo root (default: moneypenny.toml)
  --http [port]     Ensure [channels.http] exists (default port 8080)
  --start           Start gateway after re-init
  -h, --help        Show this help

Examples:
  ./scripts/reinit.sh
  ./scripts/reinit.sh --http
  ./scripts/reinit.sh --http 4821 --start
  ./scripts/reinit.sh --config moneypenny.toml --start
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config)
      CONFIG="${2:-}"
      shift 2
      ;;
    --http)
      ENABLE_HTTP=1
      if [[ "${2:-}" =~ ^[0-9]+$ ]]; then
        HTTP_PORT="$2"
        shift 2
      else
        shift
      fi
      ;;
    --start)
      START_AFTER=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo not found in PATH. Install rustup or run: source \"\$HOME/.cargo/env\""
  exit 1
fi

CONFIG_PATH="$REPO_ROOT/$CONFIG"
BACKUP_PATH="$REPO_ROOT/${CONFIG}.bak"

cd "$REPO_ROOT"

if [[ ! -f "$CONFIG_PATH" ]]; then
  echo "No config at $CONFIG_PATH. Run 'cargo run -- init' once to create it."
  exit 1
fi

echo "Backing up $CONFIG to ${CONFIG}.bak"
cp "$CONFIG_PATH" "$BACKUP_PATH"

echo "Removing data dir and config..."
rm -rf mp-data
rm -f "$CONFIG_PATH"

echo "Running mp init..."
cargo run -- init

if [[ ! -f mp-data/main.db ]]; then
  echo "Error: mp-data/main.db was not created. Check init output above."
  mv "$BACKUP_PATH" "$CONFIG_PATH"
  exit 1
fi

echo "Restoring your config..."
# init always creates the default config path (moneypenny.toml); remove it then restore backup
rm -f "$REPO_ROOT/moneypenny.toml"
mv "$BACKUP_PATH" "$CONFIG_PATH"

if [[ "$ENABLE_HTTP" -eq 1 ]]; then
  echo "Ensuring HTTP channel is enabled on port $HTTP_PORT in $CONFIG"
  if rg -n '^\[channels\.http\]' "$CONFIG_PATH" >/dev/null 2>&1; then
    tmp_file="$(mktemp)"
    awk -v p="$HTTP_PORT" '
      BEGIN { in_http=0; set_port=0 }
      /^\[channels\.http\]$/ { in_http=1; print; next }
      /^\[/ {
        if (in_http && !set_port) {
          print "port = " p
          set_port=1
        }
        in_http=0
        print
        next
      }
      {
        if (in_http && $0 ~ /^port[[:space:]]*=/) {
          print "port = " p
          set_port=1
          next
        }
        print
      }
      END {
        if (in_http && !set_port) {
          print "port = " p
        }
      }
    ' "$CONFIG_PATH" > "$tmp_file"
    mv "$tmp_file" "$CONFIG_PATH"
  else
    cat >> "$CONFIG_PATH" <<EOF

[channels.http]
port = $HTTP_PORT
EOF
  fi
fi

echo "Done."
echo "Start with: cargo run -- start"

if [[ "$START_AFTER" -eq 1 ]]; then
  echo "Starting gateway..."
  cargo run -- start
fi
