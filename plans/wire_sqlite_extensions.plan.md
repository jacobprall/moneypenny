# Plan: Wire All SQLite Extensions into the `mp` Binary (Static Compilation)

## Goal

Statically compile all 7 SQLite extensions into the `mp` Rust binary so that a single `cargo build` produces a self-contained executable with every extension auto-loaded on every connection. No `.dylib` files, no runtime `load_extension()` calls.

## Extensions (7 total)

| Extension | Language | Entry point | Key deps | Source size |
|---|---|---|---|---|
| sqlite-vector | C | `sqlite3_vector_init` | fp16 (bundled) | ~9.5K LOC |
| sqlite-ai | C (links C++) | `sqlite3_ai_init` | llama.cpp, whisper.cpp, miniaudio (submodules) | ~4K LOC + huge C++ deps |
| sqlite-sync | C | `sqlite3_cloudsync_init` | lz4 (bundled), libcurl/network | ~16K LOC |
| sqlite-js | C | `sqlite3_js_init` | QuickJS (bundled, 83K LOC) | ~1.2K LOC |
| sqlite-agent | C | `sqlite3_agent_init` | depends on sqlite-mcp + sqlite-ai at SQL level | ~1.1K LOC |
| sqlite-memory | C | `sqlite3_memory_init` | depends on sqlite-vector + optional sqlite-ai | ~30K LOC |
| sqlite-mcp | C + Rust | `sqlite3_mcp_init` | `mcp-ffi` Rust crate (rmcp, reqwest, tokio) | ~42K C + 40K Rust |

## Architecture

```
Cargo.toml (workspace)
├── crates/mp/          (binary — links everything)
├── crates/mp-core/     (Rust library)
├── crates/mp-llm/      (Rust library)
└── crates/mp-ext/      (NEW — C/C++ build crate, no Rust src)
    ├── Cargo.toml      (build-dependencies: cc, cmake)
    ├── build.rs        (compiles all C/C++ extensions into a static lib)
    ├── src/lib.rs      (extern "C" declarations + init_all_extensions())
    └── src/register.rs (calls each sqlite3_*_init via rusqlite's raw API)
```

`mp-ext` is the integration crate. Its `build.rs` compiles all C sources (and invokes CMake for llama.cpp/whisper.cpp) into static archives. Its `lib.rs` exposes a single `pub fn init_all_extensions(conn: &rusqlite::Connection)` that calls each extension's init function.

For sqlite-mcp, its Rust crate (`mcp-ffi`) becomes a workspace member, and the C shim (`sqlite-mcp.c`) gets compiled in `mp-ext/build.rs` alongside the others. The Rust FFI static lib links naturally through Cargo.

## Phased Implementation

### Phase 1: Scaffolding + Pure-C Extensions (sqlite-vector, sqlite-js, sqlite-agent)

These three are self-contained C with vendored dependencies (fp16, QuickJS, sqlite3ext.h). No CMake, no external libraries, no network deps.

**Step 1.1: Create `crates/mp-ext/` crate**
- `Cargo.toml` with `build-dependencies = { cc = "1" }` and `dependencies = { rusqlite.workspace = true }`
- `build.rs` skeleton that compiles C sources using the `cc` crate
- `src/lib.rs` with `extern "C"` declarations for each `sqlite3_*_init`
- `src/register.rs` with `init_all_extensions(conn)` that calls each init via `rusqlite::Connection::handle()` + unsafe FFI

**Step 1.2: Wire sqlite-vector**
- In `build.rs`: compile `sqlite-vector/src/*.c` + `sqlite-vector/libs/fp16/*.c` with correct include paths
- Define `SQLITE_CORE` so extension skips `SQLITE_EXTENSION_INIT1` and uses the bundled SQLite from rusqlite
- In `src/lib.rs`: declare `extern "C" { fn sqlite3_vector_init(...) -> c_int; }`
- In `src/register.rs`: call it via the raw `sqlite3*` handle from rusqlite

**Step 1.3: Wire sqlite-js**
- Compile `sqlite-js/src/sqlitejs.c` + `sqlite-js/libs/quickjs.c` (83K LOC single-file)
- QuickJS needs specific CFLAGS: `-DCONFIG_VERSION="2024-02-14"` or similar

**Step 1.4: Wire sqlite-agent**
- Compile `sqlite-agent/src/sqlite-agent.c`
- sqlite-agent calls sqlite-mcp/sqlite-ai functions via SQL at runtime (not at link time), so it compiles standalone

**Step 1.5: Update `mp-ext/Cargo.toml` as workspace member, add to `mp/Cargo.toml` deps**

**Step 1.6: Update `db.rs` — call `mp_ext::init_all_extensions(&conn)` in `configure()`**

**Step 1.7: Verify** — `cargo build` succeeds, `SELECT vector_version()`, `SELECT js_version()`, `SELECT agent_version()` return correct values via rusqlite.

### Phase 2: sqlite-sync + sqlite-memory (C with more complex deps)

**Step 2.1: Wire sqlite-sync**
- Compile all `.c` files in `sqlite-sync/src/` (cloudsync.c, dbutils.c, network.c, pk.c, utils.c, vtab.c, lz4.c)
- Network layer (`network.c`) depends on libcurl. Two options:
  - Link system libcurl (add `println!("cargo:rustc-link-lib=curl")` in build.rs)
  - Or use the `curl-sys` crate as a build dependency for static linking
- macOS: `network.m` is Objective-C (NSURLSession). Compile with `-x objective-c` flag and link `Foundation` framework
- Define `SQLITE_CORE`

**Step 2.2: Wire sqlite-memory**
- Compile all `.c` files in `sqlite-memory/src/`
- sqlite-memory uses sqlite-vector internally (via SQL: `vector_init`, `vector_full_scan`) — no link dependency, but must be initialized after sqlite-vector
- sqlite-memory's local embedding engine depends on sqlite-ai (via `llama.h`) — needs include path to llama.cpp headers
- May need `DBMEM_OMIT_LOCAL_ENGINE` flag initially if sqlite-ai isn't ready yet (uses remote embeddings only)

**Step 2.3: Update init order in `register.rs`** — extensions must init in dependency order:
1. sqlite-vector (no deps)
2. sqlite-js (no deps)
3. sqlite-mcp (no deps on other extensions)
4. sqlite-ai (no deps on other extensions)
5. sqlite-memory (depends on sqlite-vector, optionally sqlite-ai)
6. sqlite-sync (no deps on other extensions)
7. sqlite-agent (depends on sqlite-mcp + sqlite-ai at SQL level)

### Phase 3: sqlite-ai (C + massive C++ dependencies)

**Step 3.1: Initialize git submodules**
- `cd sqlite-ai && git submodule update --init --recursive`
- This pulls llama.cpp (~200K LOC C/C++), whisper.cpp, and miniaudio

**Step 3.2: Add `cmake` build dependency**
- llama.cpp and whisper.cpp use CMake. Use the `cmake` crate in `build.rs`
- Build llama.cpp as a static library with options matching the Makefile:
  - `BUILD_SHARED_LIBS=OFF`, `LLAMA_BUILD_EXAMPLES=OFF`, `LLAMA_BUILD_TESTS=OFF`
  - Metal/CUDA/NEON acceleration based on platform
- Build whisper.cpp similarly
- Build miniaudio (simpler, just C files)

**Step 3.3: Compile sqlite-ai.c**
- Needs include paths to llama.cpp, whisper.cpp, miniaudio headers
- Link against the static libs from Step 3.2

**Step 3.4: Update `build.rs`**
- Add `cc` build for `sqlite-ai/src/*.c`
- Add `cmake` invocations for llama.cpp and whisper.cpp
- Emit `cargo:rustc-link-lib=static=llama`, `cargo:rustc-link-lib=static=whisper`, etc.
- Platform-specific: link Metal framework on macOS, accelerate framework, etc.

### Phase 4: sqlite-mcp (Rust + C hybrid)

**Step 4.1: Clone sqlite-mcp into the monorepo**
- `git clone https://github.com/sqliteai/sqlite-mcp ../sqlite-mcp` (or add as submodule)
- Init its submodule: `cd sqlite-mcp && git submodule update --init --recursive` (pulls `modelcontextprotocol/rust-sdk`)

**Step 4.2: Add `mcp-ffi` as workspace member**
- Add `mcp-ffi = { path = "../sqlite-mcp" }` or create `crates/mp-mcp/` that re-exports it
- Deduplicate shared deps: both Moneypenny and mcp-ffi use `reqwest`, `tokio`, `serde_json` — workspace deps ensure single versions

**Step 4.3: Compile sqlite-mcp.c in `mp-ext/build.rs`**
- The C shim calls into the Rust `mcp_ffi` static lib via FFI
- Since `mcp-ffi` is a workspace member, Cargo links it automatically
- `build.rs` just needs to compile the `.c` file with correct include paths

**Step 4.4: Add `mcp_ffi.h` declarations to `mp-ext/src/lib.rs`**
- Declare `extern "C" { fn sqlite3_mcp_init(...) -> c_int; }`

### Phase 5: Integration + Hardening

**Step 5.1: Update `db.rs`**
- Remove the `load_extension` function (no longer needed)
- Call `mp_ext::init_all_extensions(&conn)` inside `configure()` for every connection
- Each extension's init is idempotent — safe to call per-connection

**Step 5.2: Update `open_agent_db` in `main.rs`**
- Ensure extensions are initialized before schema init
- Remove `load_extension` feature from rusqlite (no longer needed for runtime loading)

**Step 5.3: Config integration**
- Add `[extensions]` section to `moneypenny.toml` with per-extension enable/disable flags
- `mp-ext::init_extensions(conn, &config.extensions)` respects these flags

**Step 5.4: Smoke tests**
- Rust integration test that opens a connection and verifies each extension:
  ```rust
  conn.query_row("SELECT vector_version()", [], |r| r.get::<_, String>(0))?;
  conn.query_row("SELECT ai_version()", [], |r| r.get::<_, String>(0))?;
  conn.query_row("SELECT cloudsync_version()", [], |r| r.get::<_, String>(0))?;
  conn.query_row("SELECT mcp_version()", [], |r| r.get::<_, String>(0))?;
  // etc.
  ```
- Test that extension init order is correct (memory after vector, agent after mcp+ai)
- Test that `SQLITE_CORE` compilation works (extensions use `sqlite3.h` not `sqlite3ext.h`)

**Step 5.5: Feature flags**
- Add Cargo feature flags for heavy extensions:
  - `ai` (default on) — includes sqlite-ai + llama.cpp + whisper.cpp
  - `sync` (default on) — includes sqlite-sync
  - `mcp` (default on) — includes sqlite-mcp
- `cargo build --no-default-features` produces a minimal binary with just vector+js

## Key Technical Details

### SQLITE_CORE compilation
All extensions check `#ifndef SQLITE_CORE` to decide whether to use `sqlite3ext.h` (dynamic loading) or `sqlite3.h` (statically linked). We define `SQLITE_CORE` globally in `build.rs` so extensions link against the bundled SQLite from rusqlite's `bundled` feature.

### rusqlite integration
rusqlite with `bundled` feature compiles its own SQLite. We need the extensions to link against that same SQLite. The `cc` crate in `build.rs` must include rusqlite's bundled SQLite headers. We get these via:
```rust
let sqlite_include = std::env::var("DEP_SQLITE3_INCLUDE").unwrap();
```
This env var is set by rusqlite's build script when the `bundled` feature is active.

### Extension init via raw handle
```rust
use rusqlite::Connection;
use std::os::raw::{c_char, c_int};
use std::ptr;

extern "C" {
    fn sqlite3_vector_init(db: *mut std::ffi::c_void, err: *mut *mut c_char, api: *const std::ffi::c_void) -> c_int;
}

pub fn init_all_extensions(conn: &Connection) -> anyhow::Result<()> {
    unsafe {
        let db = conn.handle();
        let rc = sqlite3_vector_init(db as _, ptr::null_mut(), ptr::null());
        if rc != 0 { anyhow::bail!("sqlite-vector init failed: {rc}"); }
        // ... repeat for each extension
    }
    Ok(())
}
```

### sqlite-mcp Rust deduplication
mcp-ffi's Cargo.toml uses `reqwest` 0.12 and `tokio` 1.x — same major versions as Moneypenny. By adding it as a workspace member, Cargo resolves to a single copy. The `rmcp` crate (MCP SDK) is pulled via path from the git submodule.

## Files Created/Modified

### New files
- `crates/mp-ext/Cargo.toml`
- `crates/mp-ext/build.rs`
- `crates/mp-ext/src/lib.rs`
- `crates/mp-ext/src/register.rs`

### Modified files
- `Cargo.toml` (workspace: add `mp-ext` member, add `cc` and `cmake` to workspace deps)
- `crates/mp/Cargo.toml` (add `mp-ext` dependency)
- `crates/mp-core/src/db.rs` (call `mp_ext::init_all_extensions` in `configure()`)
- `crates/mp-core/src/config.rs` (add `ExtensionsConfig` for per-extension toggles)
- `crates/mp/src/main.rs` (pass config to db open)

## Execution Order

1. Phase 1 (vector + js + agent) — get the build system working with simple C extensions
2. Phase 2 (sync + memory) — add platform-specific C compilation
3. Phase 4 (mcp) — add the Rust+C hybrid
4. Phase 3 (ai) — add the heavy C++ deps last (CMake, llama.cpp, whisper)
5. Phase 5 (integration) — config, tests, feature flags

Phases 3 and 4 are swapped in execution because sqlite-mcp is simpler to integrate (it's already Rust) while sqlite-ai requires CMake + massive C++ compilation that's best tackled last.
