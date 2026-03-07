use rusqlite::Connection;
use std::path::Path;

/// Open a SQLite connection with standard pragmas for Moneypenny.
pub fn open(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

/// Open an in-memory SQLite database (for testing).
pub fn open_memory() -> anyhow::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        ",
    )?;
    Ok(())
}

/// Load a SQLite extension by path. The entry point is auto-detected.
pub fn load_extension(conn: &Connection, path: &Path) -> anyhow::Result<()> {
    unsafe {
        conn.load_extension(path, None)?;
    }
    Ok(())
}
