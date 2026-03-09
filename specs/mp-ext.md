# mp-ext — SQLite Extension Build & Loader Specification

> **Crate:** `crates/mp-ext/` | **Type:** Library (with build.rs) | **Dependencies:** cc, cmake (build), rusqlite, anyhow, tracing, mcp-ffi (runtime)

`mp-ext` compiles all 7 SQLite extensions from vendored C/C++ source into static libraries and links them into the Rust binary. It exposes a single function to initialize all extensions on a SQLite connection.

## Module Map

```
mp-ext/
├── Cargo.toml        # build-deps: cc, cmake. deps: rusqlite, mcp-ffi
├── build.rs          # C/C++ compilation (7 extensions)
├── src/lib.rs        # FFI declarations + init_all_extensions()
└── tests/
    └── integration.rs  # Smoke tests
```

## Extensions (7 total)

| Extension | Language | Entry Point | Submodule | Key Dependencies |
|---|---|---|---|---|
| sqlite-vector | C | `sqlite3_vector_init` | `vendor/sqlite-vector` | fp16 (bundled) |
| sqlite-js | C | `sqlite3_js_init` | `vendor/sqlite-js` | QuickJS (bundled) |
| sqlite-sync | C | `sqlite3_cloudsync_init` | `vendor/sqlite-sync` | lz4 (bundled), platform networking |
| sqlite-memory | C | `sqlite3_memory_init` | `vendor/sqlite-memory` | depends on sqlite-vector at SQL level |
| sqlite-ai | C (links C++) | `sqlite3_ai_init` | `vendor/sqlite-ai` | llama.cpp, whisper.cpp, miniaudio (nested submodules, CMake) |
| sqlite-mcp | C + Rust | `sqlite3_mcp_init` | `vendor/sqlite-mcp` | `mcp-ffi` Rust crate, MCP rust-sdk (nested submodule) |
| sqlite-agent | C | `sqlite3_agent_init` | `vendor/sqlite-agent` | depends on sqlite-mcp + sqlite-ai at SQL level |

## build.rs — Compilation

**File:** `crates/mp-ext/build.rs`

Each extension has a dedicated `build_sqlite_*()` function. All share these patterns:

- **`SQLITE_CORE` defined** — extensions compile against rusqlite's bundled SQLite headers (not `sqlite3ext.h` for dynamic loading)
- **Include path from `DEP_SQLITE3_INCLUDE`** — propagated by `libsqlite3-sys` with `bundled` feature
- **Warnings disabled** — vendored code, not ours to fix

### Build Functions

#### `build_sqlite_vector`
Compiles: `sqlite-vector.c`, `distance-cpu.c`, `distance-neon.c`, `distance-sse2.c`, `distance-avx2.c`, `distance-avx512.c`, `distance-rvv.c`. Includes fp16 headers.

#### `build_sqlite_js`
Compiles: `quickjs.c`, `sqlitejs.c`. Defines `QJS_BUILD_LIBC`.

#### `build_sqlite_sync`
Compiles: `cloudsync.c`, `dbutils.c`, `lz4.c`, `pk.c`, `vtab.c`, `utils.c`.
- **macOS/iOS:** adds `network.c` + `network.m` (Objective-C, NSURLSession), links Foundation + Security frameworks, defines `CLOUDSYNC_OMIT_CURL`.
- **Other platforms:** defines `CLOUDSYNC_OMIT_NETWORK` (CRDT merge works locally, no network).

#### `build_sqlite_memory`
Compiles: `sqlite-memory.c`, `dbmem-utils.c`, `dbmem-parser.c`, `dbmem-search.c`, `md4c.c`. Defines `DBMEM_OMIT_LOCAL_ENGINE` + `DBMEM_OMIT_REMOTE_ENGINE`.

#### `build_sqlite_mcp`
Compiles: `sqlite-mcp.c`. Force-includes `libs/sqlite3ext.h`. The C shim calls into the Rust `mcp_ffi` static library via FFI — Cargo handles linking automatically.

#### `build_sqlite_ai`
The most complex build:
1. **llama.cpp via CMake** — static libs, no shared, no examples/tests/server, Metal on macOS
2. **whisper.cpp via CMake** — uses system ggml from llama build (`WHISPER_USE_SYSTEM_GGML=ON`)
3. **miniaudio via CMake** — static, no examples/tests
4. **Link libraries:** `llama`, `ggml`, `ggml-base`, `ggml-cpu`, `mtmd`, `whisper`, `miniaudio`
5. **macOS/iOS extras:** `ggml-metal`, `ggml-blas`, Accelerate + Metal + CoreFoundation + QuartzCore frameworks
6. **C++ linking:** `libc++`
7. **Compile sqlite-ai.c** with include paths to all built headers + fp16

#### `build_sqlite_agent`
Compiles: `sqlite-agent.c`. Force-includes `libs/sqlite3ext.h`. SQL-level dependency on sqlite-mcp + sqlite-ai (not link-time).

## src/lib.rs — Extension Loader

**File:** `crates/mp-ext/src/lib.rs`

### FFI Declarations

```rust
unsafe extern "C" {
    fn sqlite3_vector_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
    fn sqlite3_js_init(...) -> c_int;
    fn sqlite3_cloudsync_init(...) -> c_int;
    fn sqlite3_memory_init(...) -> c_int;
    fn sqlite3_ai_init(...) -> c_int;
    fn sqlite3_mcp_init(...) -> c_int;
    fn sqlite3_agent_init(...) -> c_int;
}
```

`extern crate mcp_ffi;` forces the linker to include mcp-ffi symbols.

### `init_all_extensions(conn: &rusqlite::Connection) -> anyhow::Result<()>`

Initializes all extensions on a connection in dependency order:

1. **Phase 1 (no deps):** sqlite-vector, sqlite-js, sqlite-sync
2. **Phase 2 (depends on vector):** sqlite-memory
3. **Phase 3 (network/inference):** sqlite-mcp, sqlite-ai
4. **Phase 4 (depends on mcp+ai):** sqlite-agent

Each init call goes through `call_init()` which checks the return code and extracts any error message from the C error pointer.

Safe to call multiple times per connection — each extension's init is idempotent.

## Integration Tests

**File:** `crates/mp-ext/tests/integration.rs`

| Test | What it verifies |
|---|---|
| `init_all_extensions_succeeds` | All 7 extensions load without error |
| `init_is_idempotent` | Double-init doesn't fail |
| `js_eval_works` | `SELECT js_eval('1 + 2')` returns 3 |
| `vector_version_exists` | `SELECT vector_version()` returns non-empty |

## Workspace Integration

- **Workspace `Cargo.toml`** excludes `vendor/` so Cargo doesn't auto-discover `sqlite-mcp` as a workspace member
- **`mcp-ffi`** is a path dependency: `../../vendor/sqlite-mcp`
- **`libsqlite3-sys`** is an explicit dep (with `bundled` feature) so `DEP_SQLITE3_INCLUDE` is available in build.rs
- **Shared deps** (reqwest, tokio, serde_json) deduplicated via Cargo resolver v2

## Fetching Submodules

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
# or after a shallow clone:
git submodule update --init --recursive
```

`build.rs` panics with a helpful message if `vendor/` is missing.
