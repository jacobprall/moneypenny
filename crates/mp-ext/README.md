# mp-ext

`mp-ext` statically links and initializes bundled SQLite extensions required by Moneypenny.

## What this crate does

- Compiles C-based SQLite extensions through `build.rs`.
- Exposes a single Rust API to initialize all linked extensions on a `rusqlite::Connection`.
- Enforces initialization order so extension dependencies are loaded safely.
- Provides the bridge for MCP/agent-related extension symbols at link time.

## Key files

- `src/lib.rs`: extension init entrypoint (`init_all_extensions`).
- `build.rs`: native build and static link configuration for extension sources.

## Usage

```rust
let conn = rusqlite::Connection::open("mp-data/main.db")?;
mp_ext::init_all_extensions(&conn)?;
```

Call initialization once per opened connection before executing extension-dependent SQL.

## Test and build

From the workspace root:

```bash
cargo check -p mp-ext
```

`mp-ext` is primarily validated through integration paths in `mp` and `mp-core`.
