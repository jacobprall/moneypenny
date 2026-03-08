#!/usr/bin/env bash
# Full Moneypenny setup: build, download models, initialize, verify.
#
# Idempotent — safe to re-run. Skips steps that are already done.
#
# Usage:
#   ./scripts/setup.sh              # full setup with defaults
#   ./scripts/setup.sh --clean      # wipe mp-data/ first, then setup
#   ./scripts/setup.sh --quant Q4_K_M   # use a smaller quantization
#   ./scripts/setup.sh --skip-build # skip cargo build (if you already built)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
CLEAN=0
SKIP_BUILD=0
QUANT="Q8_0"
MODEL_NAME="nomic-embed-text-v1.5"
HF_REPO="nomic-ai/nomic-embed-text-v1.5-GGUF"
DATA_DIR="mp-data"
MODELS_DIR="$DATA_DIR/models"
CONFIG_FILE="moneypenny.toml"

# ---------------------------------------------------------------------------
# Parse args
# ---------------------------------------------------------------------------
usage() {
  cat <<'EOF'
Usage: scripts/setup.sh [options]

Options:
  --clean            Remove mp-data/ and moneypenny.toml before setup
  --skip-build       Skip cargo build (use existing binary)
  --quant <QUANT>    GGUF quantization level (default: Q8_0)
                     Common: Q4_K_M (~137MB), Q8_0 (~274MB), f16 (~530MB)
  --data-dir <DIR>   Data directory (default: mp-data)
  --config <FILE>    Config file (default: moneypenny.toml)
  -h, --help         Show this help

Examples:
  ./scripts/setup.sh
  ./scripts/setup.sh --clean
  ./scripts/setup.sh --quant Q4_K_M
  ./scripts/setup.sh --clean --quant Q4_K_M --skip-build
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean)       CLEAN=1; shift ;;
    --skip-build)  SKIP_BUILD=1; shift ;;
    --quant)       QUANT="${2:?--quant requires a value}"; shift 2 ;;
    --data-dir)    DATA_DIR="${2:?--data-dir requires a value}"; MODELS_DIR="$DATA_DIR/models"; shift 2 ;;
    --config)      CONFIG_FILE="${2:?--config requires a value}"; shift 2 ;;
    -h|--help)     usage; exit 0 ;;
    *)             echo "Unknown option: $1"; usage; exit 1 ;;
  esac
done

# The GGUF file on HuggingFace vs. what the config expects
GGUF_REMOTE_NAME="${MODEL_NAME}.${QUANT}.gguf"
GGUF_LOCAL_NAME="${MODEL_NAME}.gguf"
GGUF_URL="https://huggingface.co/${HF_REPO}/resolve/main/${GGUF_REMOTE_NAME}"
GGUF_PATH="${MODELS_DIR}/${GGUF_LOCAL_NAME}"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
info()  { printf "\033[1;34m==>\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m  ✓\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33m  !\033[0m %s\n" "$*"; }
fail()  { printf "\033[1;31m  ✗\033[0m %s\n" "$*"; exit 1; }

# ---------------------------------------------------------------------------
# 1. Prerequisites
# ---------------------------------------------------------------------------
info "Checking prerequisites"

command -v cargo >/dev/null 2>&1 || fail "cargo not found. Install Rust: https://rustup.rs"
ok "cargo $(cargo --version | awk '{print $2}')"

if command -v curl >/dev/null 2>&1; then
  DOWNLOADER="curl"
  ok "curl available"
elif command -v wget >/dev/null 2>&1; then
  DOWNLOADER="wget"
  ok "wget available"
else
  fail "Neither curl nor wget found. Install one to download models."
fi

# ---------------------------------------------------------------------------
# 2. Git submodules
# ---------------------------------------------------------------------------
info "Checking git submodules"

if [[ -f ".gitmodules" ]]; then
  # Check if any submodule dir is empty (not initialized)
  NEED_SUBMODULES=0
  while IFS= read -r subpath; do
    if [[ -n "$subpath" && ! -f "$subpath/.git" && ! -d "$subpath/.git" ]]; then
      NEED_SUBMODULES=1
      break
    fi
  done < <(git config --file .gitmodules --get-regexp path | awk '{print $2}')

  if [[ "$NEED_SUBMODULES" -eq 1 ]]; then
    info "Initializing git submodules (this may take a moment)"
    git submodule update --init --recursive
    ok "Submodules initialized"
  else
    ok "Submodules already initialized"
  fi
else
  ok "No submodules"
fi

# ---------------------------------------------------------------------------
# 3. Clean (optional)
# ---------------------------------------------------------------------------
if [[ "$CLEAN" -eq 1 ]]; then
  info "Cleaning previous data"
  rm -rf "$DATA_DIR"
  rm -f "$CONFIG_FILE"
  ok "Removed $DATA_DIR/ and $CONFIG_FILE"
fi

# ---------------------------------------------------------------------------
# 4. Build
# ---------------------------------------------------------------------------
if [[ "$SKIP_BUILD" -eq 0 ]]; then
  info "Building moneypenny (cargo build)"
  cargo build 2>&1 | tail -5
  ok "Build complete — binary at target/debug/mp"
else
  if [[ ! -x "target/debug/mp" ]]; then
    fail "target/debug/mp not found. Remove --skip-build or run cargo build first."
  fi
  ok "Skipping build (--skip-build)"
fi

MP="./target/debug/mp"

# ---------------------------------------------------------------------------
# 5. Download embedding model
# ---------------------------------------------------------------------------
info "Checking embedding model"

mkdir -p "$MODELS_DIR"

if [[ -f "$GGUF_PATH" ]]; then
  FILE_SIZE=$(wc -c < "$GGUF_PATH" | tr -d ' ')
  if [[ "$FILE_SIZE" -gt 1000000 ]]; then
    ok "Model already present: $GGUF_PATH ($(numfmt --to=iec "$FILE_SIZE" 2>/dev/null || echo "${FILE_SIZE} bytes"))"
  else
    warn "Model file exists but looks too small (${FILE_SIZE} bytes). Re-downloading."
    rm -f "$GGUF_PATH"
  fi
fi

if [[ ! -f "$GGUF_PATH" ]]; then
  info "Downloading ${GGUF_REMOTE_NAME} from HuggingFace"
  echo "    URL: $GGUF_URL"
  echo "    Dest: $GGUF_PATH"
  echo ""

  TEMP_PATH="${GGUF_PATH}.part"

  if [[ "$DOWNLOADER" == "curl" ]]; then
    curl -L --progress-bar -o "$TEMP_PATH" "$GGUF_URL"
  else
    wget --show-progress -O "$TEMP_PATH" "$GGUF_URL"
  fi

  # Validate download
  if [[ ! -f "$TEMP_PATH" ]]; then
    fail "Download failed — no file created."
  fi

  DL_SIZE=$(wc -c < "$TEMP_PATH" | tr -d ' ')
  if [[ "$DL_SIZE" -lt 1000000 ]]; then
    rm -f "$TEMP_PATH"
    fail "Downloaded file is too small (${DL_SIZE} bytes). The URL may be wrong or HuggingFace returned an error. Try a different --quant value."
  fi

  mv "$TEMP_PATH" "$GGUF_PATH"
  ok "Model downloaded: $GGUF_PATH ($(numfmt --to=iec "$DL_SIZE" 2>/dev/null || echo "${DL_SIZE} bytes"))"
fi

# ---------------------------------------------------------------------------
# 6. Initialize moneypenny
# ---------------------------------------------------------------------------
info "Initializing moneypenny"

if [[ -f "$CONFIG_FILE" && -f "$DATA_DIR/main.db" ]]; then
  ok "Already initialized ($CONFIG_FILE and $DATA_DIR/main.db exist)"
  ok "To re-initialize, run with --clean"
else
  # Remove partial state so mp init doesn't fail
  rm -f "$CONFIG_FILE"

  $MP init
  ok "Initialized"
fi

# ---------------------------------------------------------------------------
# 7. Verify
# ---------------------------------------------------------------------------
info "Verifying installation"

if [[ ! -f "$CONFIG_FILE" ]]; then
  fail "Config file not created: $CONFIG_FILE"
fi
ok "Config: $CONFIG_FILE"

if [[ ! -f "$DATA_DIR/main.db" ]]; then
  fail "Agent database not created: $DATA_DIR/main.db"
fi
ok "Database: $DATA_DIR/main.db"

if [[ ! -f "$GGUF_PATH" ]]; then
  fail "Model not found: $GGUF_PATH"
fi
ok "Model: $GGUF_PATH"

# Quick smoke test: run facts list to confirm schema + extensions load
if $MP facts list main >/dev/null 2>&1; then
  ok "Smoke test passed (mp facts list main)"
else
  warn "Smoke test failed — 'mp facts list main' returned non-zero. Check output above."
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
echo ""
info "Setup complete!"
echo ""
echo "  Next steps:"
echo "    mp start               # start the gateway"
echo "    mp chat main           # interactive chat"
echo "    mp facts list main     # list stored facts"
echo ""
echo "  Useful scripts:"
echo "    scripts/reinit.sh      # wipe and re-initialize (keeps config)"
echo "    scripts/setup.sh -h    # see all options"
echo ""
