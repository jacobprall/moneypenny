#!/usr/bin/env bash
# ============================================================================
# Moneypenny — Fresh setup and chat
#
# One command to: clean, download, build, init, verify, and open chat.
# Use this when you're tired of running the same setup steps.
#
# Usage:
#   ./scripts/fresh.sh              # full clean setup + chat
#   ./scripts/fresh.sh --no-chat    # setup only, don't open chat
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

AUTO_CHAT=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-chat) AUTO_CHAT=0; shift ;;
    -h|--help)
      echo "Usage: scripts/fresh.sh [--no-chat]"
      echo ""
      echo "  --no-chat   Setup only, don't open chat"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
DATA_DIR="mp-data"
MODELS_DIR="$DATA_DIR/models"
CONFIG_FILE="moneypenny.toml"
MODEL_NAME="nomic-embed-text-v1.5"
QUANT="Q4_K_M"
GGUF_REMOTE_NAME="${MODEL_NAME}.${QUANT}.gguf"
GGUF_LOCAL_NAME="${MODEL_NAME}.gguf"
GGUF_URL="https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/${GGUF_REMOTE_NAME}"
GGUF_PATH="${MODELS_DIR}/${GGUF_LOCAL_NAME}"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
info()  { printf "\033[1;34m==>\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m  ✓\033[0m %s\n" "$*"; }
fail()  { printf "\033[1;31m  ✗\033[0m %s\n" "$*"; exit 1; }

# ---------------------------------------------------------------------------
# 1. Prerequisites
# ---------------------------------------------------------------------------
info "Checking prerequisites"
command -v cargo >/dev/null 2>&1 || fail "cargo not found. Install Rust: https://rustup.rs"
ok "cargo $(cargo --version | awk '{print $2}')"

if command -v curl >/dev/null 2>&1; then
  DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
  DOWNLOADER="wget"
else
  fail "Neither curl nor wget found. Install one to download models."
fi

# ---------------------------------------------------------------------------
# 2. Git submodules
# ---------------------------------------------------------------------------
info "Initializing git submodules"
if [[ -f ".gitmodules" ]]; then
  git submodule update --init --recursive 2>/dev/null || true
fi
ok "Submodules ready"

# ---------------------------------------------------------------------------
# 3. Clean
# ---------------------------------------------------------------------------
info "Cleaning previous data"
rm -rf "$DATA_DIR"
rm -f "$CONFIG_FILE"
ok "Removed $DATA_DIR/ and $CONFIG_FILE"

# ---------------------------------------------------------------------------
# 4. Build (clean first for a truly fresh build)
# ---------------------------------------------------------------------------
info "Building moneypenny (release)"
cargo clean
cargo build --release 2>&1 | tail -8
ok "Build complete"

MP="./target/release/mp"

# ---------------------------------------------------------------------------
# 5. Download embedding model
# ---------------------------------------------------------------------------
info "Downloading embedding model (${QUANT}, ~137MB)"
mkdir -p "$MODELS_DIR"

TEMP_PATH="${GGUF_PATH}.part"
if [[ "$DOWNLOADER" == "curl" ]]; then
  curl -L --progress-bar -o "$TEMP_PATH" "$GGUF_URL"
else
  wget --show-progress -O "$TEMP_PATH" "$GGUF_URL"
fi

if [[ ! -f "$TEMP_PATH" ]]; then
  fail "Download failed — no file created."
fi

DL_SIZE=$(wc -c < "$TEMP_PATH" | tr -d ' ')
if [[ "$DL_SIZE" -lt 1000000 ]]; then
  rm -f "$TEMP_PATH"
  fail "Downloaded file too small (${DL_SIZE} bytes). Check network or try again."
fi

mv "$TEMP_PATH" "$GGUF_PATH"
ok "Model: $GGUF_PATH ($(numfmt --to=iec "$DL_SIZE" 2>/dev/null || echo "${DL_SIZE} bytes"))"

# ---------------------------------------------------------------------------
# 6. Initialize
# ---------------------------------------------------------------------------
info "Initializing moneypenny"
$MP init
ok "Initialized"

# ---------------------------------------------------------------------------
# 7. Verify
# ---------------------------------------------------------------------------
info "Verifying"
$MP doctor 2>&1 | tail -15
ok "Doctor complete"

# ---------------------------------------------------------------------------
# 8. Chat (or done)
# ---------------------------------------------------------------------------
echo ""
if [[ "$AUTO_CHAT" -eq 1 ]]; then
  info "Opening chat..."
  echo ""
  exec $MP chat
else
  info "Setup complete!"
  echo ""
  echo "  Run:  $MP chat"
  echo ""
fi
