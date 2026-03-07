# Plan: Wire All SQLite Extensions into the `mp` Binary (Static Compilation)

## Goal

Statically compile all 7 SQLite extensions into the `mp` Rust binary so that a single `cargo build` produces a self-contained executable with every extension auto-loaded on every connection. No `.dylib` files, no runtime `load_extension()` calls.

## Source Management

All extension source code is vendored as **git submodules** under `vendor/`:

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
# or, after a shallow clone:
git submodule update --init --recursive
```

This ensures anyone cloning the repo gets all extension sources without needing sibling repos, a monorepo layout, or manual setup.

## Extensions (7 total)

| Extension | Language | Entry point | Submodule | Key deps |
|---|---|---|---|---|
| sqlite-vector | C | `sqlite3_vector_init` | `vendor/sqlite-vector` | fp16 (bundled) |
| sqlite-js | C | `sqlite3_js_init` | `vendor/sqlite-js` | QuickJS (bundled) |
| sqlite-sync | C | `sqlite3_cloudsync_init` | `vendor/sqlite-sync` | lz4 (bundled), platform networking |
| sqlite-memory | C | `sqlite3_memory_init` | `vendor/sqlite-memory` | depends on sqlite-vector at SQL level |
| sqlite-ai | C (links C++) | `sqlite3_ai_init` | `vendor/sqlite-ai` | llama.cpp, whisper.cpp, miniaudio (nested submodules) |
| sqlite-mcp | C + Rust | `sqlite3_mcp_init` | `vendor/sqlite-mcp` | `mcp-ffi` Rust crate, MCP rust-sdk (nested submodule) |
| sqlite-agent | C | `sqlite3_agent_init` | `vendor/sqlite-agent` | depends on sqlite-mcp + sqlite-ai at SQL level |

## Architecture

```
Cargo.toml (workspace, excludes vendor/)
├── crates/mp/          (binary — links everything)
├── crates/mp-core/     (Rust library)
├── crates/mp-llm/      (Rust library)
├── crates/mp-ext/      (C/C++ build crate)
│   ├── Cargo.toml      (build-deps: cc, cmake; deps: mcp-ffi path)
│   ├── build.rs        (compiles all C/C++ from vendor/ into static libs)
│   └── src/lib.rs      (extern "C" declarations + init_all_extensions())
└── vendor/             (git submodules — extension sources)
    ├── sqlite-vector/
    ├── sqlite-js/
    ├── sqlite-sync/
    ├── sqlite-memory/
    ├── sqlite-ai/      (has nested submodules: llama.cpp, whisper.cpp, miniaudio)
    ├── sqlite-mcp/     (has nested submodule: MCP rust-sdk; is also a Rust crate)
    └── sqlite-agent/
```

`mp-ext` is the integration crate. Its `build.rs` compiles all C sources from `vendor/` (and invokes CMake for llama.cpp/whisper.cpp) into static archives. Its `lib.rs` exposes `pub fn init_all_extensions(conn: &rusqlite::Connection)`.

For sqlite-mcp, the `mcp-ffi` Rust crate is referenced as a path dependency from `vendor/sqlite-mcp`. The workspace `Cargo.toml` excludes `vendor/` so Cargo doesn't auto-discover it as a workspace member.

## Phased Implementation

### Phase 1: Pure-C Extensions (sqlite-vector, sqlite-js, sqlite-agent)

Self-contained C with vendored dependencies. No CMake, no external libraries, no network deps.

**Step 1.1: Verify submodules are populated**
- `build.rs` checks `vendor/` exists and panics with a helpful message if not
- Each build function checks the extension dir exists before compiling

**Step 1.2: Wire sqlite-vector**
- `build.rs`: compile `vendor/sqlite-vector/src/*.c` + fp16 headers
- Define `SQLITE_CORE` so extension uses bundled SQLite from rusqlite

**Step 1.3: Wire sqlite-js**
- Compile `vendor/sqlite-js/src/sqlitejs.c` + `vendor/sqlite-js/libs/quickjs.c`

**Step 1.4: Wire sqlite-agent**
- Compile `vendor/sqlite-agent/src/sqlite-agent.c`
- Depends on sqlite-mcp + sqlite-ai at SQL level only (not at link time)

**Step 1.5: Verify** — `cargo build` succeeds, `SELECT vector_version()`, `SELECT js_version()`, `SELECT agent_version()` return correct values.

### Phase 2: sqlite-sync + sqlite-memory (C with platform deps)

**Step 2.1: Wire sqlite-sync**
- Compile all `.c` files from `vendor/sqlite-sync/src/`
- macOS/iOS: `network.m` (Objective-C, NSURLSession) + Foundation/Security frameworks
- Other platforms: `CLOUDSYNC_OMIT_NETWORK` (CRDT merge still works locally)

**Step 2.2: Wire sqlite-memory**
- Compile from `vendor/sqlite-memory/src/`
- Must init after sqlite-vector; initially built with `DBMEM_OMIT_LOCAL_ENGINE` + `DBMEM_OMIT_REMOTE_ENGINE`

### Phase 3: sqlite-ai (C + massive C++ dependencies)

**Step 3.1: Ensure nested submodules are initialized**
- `git submodule update --init --recursive` fetches llama.cpp, whisper.cpp, miniaudio inside `vendor/sqlite-ai/modules/`

**Step 3.2: CMake builds**
- llama.cpp and whisper.cpp via the `cmake` crate in `build.rs`
- miniaudio via CMake
- Platform-specific: Metal on macOS, NEON on ARM, etc.

**Step 3.3: Compile sqlite-ai.c**
- Include paths to llama.cpp, whisper.cpp, miniaudio headers from the CMake build outputs
- fp16 headers from `vendor/sqlite-vector/libs/fp16`

### Phase 4: sqlite-mcp (Rust + C hybrid)

**Step 4.1: mcp-ffi as path dependency**
- `mp-ext/Cargo.toml`: `mcp-ffi = { path = "../../vendor/sqlite-mcp" }`
- Workspace `Cargo.toml`: `exclude = ["vendor"]` prevents auto-discovery
- Shared deps (reqwest, tokio, serde_json) deduplicated via Cargo resolver

**Step 4.2: Compile sqlite-mcp.c**
- The C shim calls into the Rust `mcp_ffi` static lib via FFI
- Cargo links the Rust crate automatically; `build.rs` compiles the C file

**Step 4.3: Nested submodule**
- sqlite-mcp has `modules/mcp` (modelcontextprotocol/rust-sdk) — fetched by `--recurse-submodules`

### Phase 5: Integration + Hardening

**Step 5.1: Feature flags**
- `ai` (default on) — includes sqlite-ai + llama.cpp + whisper.cpp
- `sync` (default on) — includes sqlite-sync
- `mcp` (default on) — includes sqlite-mcp
- `cargo build --no-default-features` produces a minimal binary with just vector+js

**Step 5.2: Smoke tests**
- Verify each extension loads: `SELECT vector_version()`, `SELECT ai_version()`, etc.
- Verify init order (memory after vector, agent after mcp+ai)
- Verify `SQLITE_CORE` compilation (extensions use `sqlite3.h` not `sqlite3ext.h`)

## Key Technical Details

### SQLITE_CORE compilation
All extensions check `#ifndef SQLITE_CORE` to decide whether to use `sqlite3ext.h` (dynamic loading) or `sqlite3.h` (statically linked). We define `SQLITE_CORE` in `build.rs` so extensions link against rusqlite's bundled SQLite.

### rusqlite integration
rusqlite with `bundled` feature compiles its own SQLite. The `cc` crate in `build.rs` includes rusqlite's bundled SQLite headers via:
```rust
let sqlite_include = std::env::var("DEP_SQLITE3_INCLUDE").unwrap();
```

### Extension init via raw handle
```rust
pub fn init_all_extensions(conn: &Connection) -> anyhow::Result<()> {
    unsafe {
        let db = conn.handle() as *mut c_void;
        call_init(db, sqlite3_vector_init, "sqlite-vector")?;
        call_init(db, sqlite3_js_init, "sqlite-js")?;
        call_init(db, sqlite3_cloudsync_init, "sqlite-sync")?;
        call_init(db, sqlite3_memory_init, "sqlite-memory")?;
        call_init(db, sqlite3_mcp_init, "sqlite-mcp")?;
        call_init(db, sqlite3_ai_init, "sqlite-ai")?;
        call_init(db, sqlite3_agent_init, "sqlite-agent")?;
    }
    Ok(())
}
```

## Execution Order

1. Phase 1 (vector + js + agent) — get the build system working with simple C extensions
2. Phase 2 (sync + memory) — add platform-specific C compilation
3. Phase 4 (mcp) — add the Rust+C hybrid
4. Phase 3 (ai) — add the heavy C++ deps last (CMake, llama.cpp, whisper)
5. Phase 5 (integration) — feature flags, smoke tests

Phases 3 and 4 are swapped in execution because sqlite-mcp is simpler to integrate (already Rust) while sqlite-ai requires CMake + massive C++ compilation.
