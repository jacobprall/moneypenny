//! Static extension loader for all SQLite extensions bundled into the `mp` binary.
//!
//! Each extension is compiled from C source via `build.rs` and registered here
//! through the raw `sqlite3*` handle from rusqlite. Extensions are initialized
//! in dependency order — sqlite-vector before sqlite-memory, sqlite-mcp before
//! sqlite-ai, etc.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;

// Force the linker to include mcp-ffi symbols (the Rust crate that sqlite-mcp.c calls into).
extern crate mcp_ffi;

unsafe extern "C" {
    fn sqlite3_vector_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
    fn sqlite3_js_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
    fn sqlite3_cloudsync_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
    fn sqlite3_memory_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
    fn sqlite3_ai_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
    fn sqlite3_mcp_init(db: *mut c_void, err: *mut *mut c_char, api: *const c_void) -> c_int;
}

/// Initialize all statically-linked SQLite extensions on the given connection.
///
/// Must be called once per connection, after the connection is opened but before
/// any extension-dependent SQL is executed. Safe to call multiple times (each
/// extension's init is idempotent).
pub fn init_all_extensions(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    unsafe {
        let db = conn.handle() as *mut c_void;

        // Phase 1: no-dependency extensions
        call_init(db, sqlite3_vector_init, "sqlite-vector")?;
        call_init(db, sqlite3_js_init, "sqlite-js")?;
        call_init(db, sqlite3_cloudsync_init, "sqlite-sync")?;

        // Phase 2: extensions that depend on sqlite-vector
        call_init(db, sqlite3_memory_init, "sqlite-memory")?;

        // Phase 3: extensions with network/MCP and AI inference
        call_init(db, sqlite3_mcp_init, "sqlite-mcp")?;
        call_init(db, sqlite3_ai_init, "sqlite-ai")?;
    }

    tracing::debug!("all SQLite extensions initialized");
    Ok(())
}

unsafe fn call_init(
    db: *mut c_void,
    init_fn: unsafe extern "C" fn(*mut c_void, *mut *mut c_char, *const c_void) -> c_int,
    name: &str,
) -> anyhow::Result<()> {
    let mut err: *mut c_char = ptr::null_mut();
    let rc = unsafe { init_fn(db, &mut err, ptr::null()) };
    if rc != 0 {
        let msg = if !err.is_null() {
            unsafe { std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned() }
        } else {
            format!("error code {rc}")
        };
        anyhow::bail!("{name} init failed: {msg}");
    }
    tracing::trace!("{name} initialized");
    Ok(())
}
