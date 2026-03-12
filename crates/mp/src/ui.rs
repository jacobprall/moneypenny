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

const BANNER_ASCII: &str = r#"
  ╔═══════════════════════════════════════╗
  ║     M O N E Y P E N N Y               ║
  ╚═══════════════════════════════════════╝
"#;

pub fn banner() {
    blank();
    if styled() {
        println!("{}  v{}", BANNER_ASCII, env!("CARGO_PKG_VERSION").cyan());
    } else {
        println!("{}  v{}", BANNER_ASCII, env!("CARGO_PKG_VERSION"));
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

/// Print a table row with aligned columns (matches table_header).
pub fn row(cols: &[(&str, usize)]) {
    let line: String = cols
        .iter()
        .map(|(val, width)| format!("{:<width$}", val))
        .collect::<Vec<_>>()
        .join(" ");
    println!("  {line}");
}

/// Create a spinner for long-running operations. Renders on stderr.
/// Call `finish_and_clear()` before printing results to stdout.
pub fn spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.dim} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Render markdown to terminal. Respects NO_COLOR and styled().
/// When styling is disabled, returns raw text.
pub fn render_markdown(text: &str) {
    if !styled() {
        for line in text.lines() {
            println!("  {line}");
        }
        return;
    }
    let skin = termimad::MadSkin::default();
    let (width, _) = termimad::terminal_size();
    let width = width.saturating_sub(4) as usize;
    let fmt = termimad::FmtText::from(&skin, text, Some(width));
    let output = format!("{fmt}");
    for line in output.lines() {
        println!("  {line}");
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
