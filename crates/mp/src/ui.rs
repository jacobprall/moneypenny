use owo_colors::OwoColorize;
use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

pub fn styled() -> bool {
    static STYLED: OnceLock<bool> = OnceLock::new();
    *STYLED.get_or_init(|| {
        std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal()
    })
}

pub fn banner() {
    blank();
    if styled() {
        println!("  {} v{}", "Moneypenny".bold(), env!("CARGO_PKG_VERSION"));
    } else {
        println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    }
    blank();
}

pub fn success(msg: impl Display) {
    if styled() {
        println!("  {} {msg}", "✓".green());
    } else {
        println!("  ✓ {msg}");
    }
}

pub fn warn(msg: impl Display) {
    if styled() {
        println!("  {} {msg}", "!".yellow());
    } else {
        println!("  ! {msg}");
    }
}

pub fn error(msg: impl Display) {
    if styled() {
        eprintln!("  {} {msg}", "✗".red());
    } else {
        eprintln!("  ✗ {msg}");
    }
}

pub fn info(msg: impl Display) {
    println!("  {msg}");
}

pub fn detail(msg: impl Display) {
    println!("      {msg}");
}

pub fn hint(msg: impl Display) {
    println!("    {msg}");
}

pub fn dim(msg: impl Display) {
    if styled() {
        println!("  {}", msg.to_string().dimmed());
    } else {
        println!("  {msg}");
    }
}

/// Print a labeled field with aligned value.
///
/// `width` controls the total column width of `"key:"` (including padding).
/// Use the same width for a group of fields to align their values.
pub fn field(key: &str, width: usize, value: impl Display) {
    let label = format!("{key}:");
    if styled() {
        println!("  {:<width$}{value}", label.dimmed());
    } else {
        println!("  {:<width$}{value}", label);
    }
}

/// Print a dimmed table header row and a thin horizontal rule.
pub fn table_header(cols: &[(&str, usize)]) {
    let header: String = cols
        .iter()
        .map(|(name, width)| format!("{:<width$}", name.to_uppercase()))
        .collect::<Vec<_>>()
        .join(" ");
    let rule: String = cols
        .iter()
        .map(|(_, width)| "─".repeat(*width))
        .collect::<Vec<_>>()
        .join(" ");
    if styled() {
        println!("  {}", header.dimmed());
        println!("  {}", rule.dimmed());
    } else {
        println!("  {header}");
        println!("  {rule}");
    }
}

pub fn blank() {
    println!();
}

pub fn prompt() {
    if styled() {
        print!("  {} ", ">".dimmed());
    } else {
        print!("  > ");
    }
    flush();
}

pub fn flush() {
    let _ = std::io::stdout().flush();
}
