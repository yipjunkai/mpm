// UI module for consistent terminal output with progress bars and styling
//
// This module provides uv/pnpm-style terminal output with spinners and progress bars.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use console::{Style, Term, style};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

/// Spinner style similar to uv/pnpm
const SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";

/// Check if stderr is a TTY (for interactive output)
fn is_tty() -> bool {
    Term::stderr().is_term()
}

/// Create a styled spinner for async operations
pub fn spinner(message: &str) -> ProgressBar {
    let pb = if is_tty() {
        ProgressBar::new_spinner()
    } else {
        // In non-TTY mode, use a hidden draw target
        // We'll print messages directly instead
        let pb = ProgressBar::new_spinner();
        pb.set_draw_target(ProgressDrawTarget::hidden());
        pb
    };

    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars(SPINNER_CHARS)
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());

    if is_tty() {
        pb.enable_steady_tick(Duration::from_millis(80));
    }

    pb
}

/// Create a progress bar for downloads with size
#[allow(dead_code)]
pub fn download_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.cyan} {msg} [{bar:25.cyan/dim}] {bytes}/{total_bytes} ({bytes_per_sec})",
            )
            .unwrap()
            .tick_chars(SPINNER_CHARS)
            .progress_chars("━━╺"),
    );
    pb
}

/// Create an indeterminate progress bar (when size is unknown)
#[allow(dead_code)]
pub fn download_bar_indeterminate() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars(SPINNER_CHARS)
            .template("{spinner:.cyan} {msg} {bytes} ({bytes_per_sec})")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Create a multi-progress bar manager
#[allow(dead_code)]
pub fn multi_progress() -> MultiProgress {
    MultiProgress::new()
}

/// Styles for different message types
#[allow(dead_code)]
pub struct Styles {
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    pub info: Style,
    pub dim: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self::new()
    }
}

impl Styles {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            success: Style::new().green(),
            warning: Style::new().yellow(),
            error: Style::new().red(),
            info: Style::new().cyan(),
            dim: Style::new().dim(),
        }
    }
}

/// Print a success message with checkmark
pub fn success(message: &str) {
    println!("{} {}", style("✓").green(), message);
}

/// Print an info/action message with arrow
pub fn action(message: &str) {
    println!("{} {}", style("→").cyan(), message);
}

/// Print a warning message
pub fn warning(message: &str) {
    eprintln!("{} {}", style("⚠").yellow(), message);
}

/// Print an error message
pub fn error(message: &str) {
    eprintln!("{} {}", style("✗").red(), message);
}

/// Print a header/section message
#[allow(dead_code)]
pub fn header(message: &str) {
    println!("{}", style(message).bold());
}

/// Print a dimmed/secondary message
pub fn dim(message: &str) {
    println!("{}", style(message).dim());
}

/// Print a status message (for dry-run, etc.)
pub fn status(prefix: &str, message: &str) {
    println!("{} {}", style(prefix).cyan().bold(), message);
}

/// Finish a spinner with success
#[allow(dead_code)]
pub fn finish_spinner_success(pb: &ProgressBar, message: &str) {
    let msg = format!("{} {}", style("✓").green(), message);
    if is_tty() {
        pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        pb.finish_with_message(msg);
    } else {
        pb.finish_and_clear();
        println!("{}", msg);
    }
}

/// Finish a spinner with the resolved version info
pub fn finish_spinner_resolved(pb: &ProgressBar, name: &str, version: &str) {
    let msg = format!("{} {} {}", style("✓").green(), name, style(version).dim());
    if is_tty() {
        pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        pb.finish_with_message(msg);
    } else {
        pb.finish_and_clear();
        println!("{}", msg);
    }
}

/// Finish a spinner with error
pub fn finish_spinner_error(pb: &ProgressBar, message: &str) {
    let msg = format!("{} {}", style("✗").red(), message);
    if is_tty() {
        pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        pb.finish_with_message(msg);
    } else {
        pb.finish_and_clear();
        eprintln!("{}", msg);
    }
}

/// Finish a download bar with success
pub fn finish_download_success(pb: &ProgressBar, name: &str) {
    let msg = format!(
        "{} {} {}",
        style("✓").green(),
        name,
        style("verified").dim()
    );
    if is_tty() {
        pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        pb.finish_with_message(msg);
    } else {
        pb.finish_and_clear();
        println!("{}", msg);
    }
}

/// Clear a progress bar without leaving a message
pub fn clear_bar(pb: &ProgressBar) {
    pb.finish_and_clear();
}
