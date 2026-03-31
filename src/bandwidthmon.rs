// File: src/bandwidthmon.rs
//! Bandwidth Monitor - robust, production-ready, cross-platform
//! Author: Hadi Cahyadi <cumulus13@gmail.com>
//! License: MIT
//! [package]
//! name = "bandwidthmon"
//! version = "0.1.17"
//! edition = "2024"
//! rust-version = "1.86"
//! authors = [
//!     "Hadi Cahyadi <cumulus13@gmail.com>",
//! ]
//! description = "Real-time network bandwidth monitor with beautiful ASCII charts"
//! license = "MIT"
//! repository = "https://!github.com/cumulus13/bandwidthmon"
//! homepage = "https://!github.com/cumulus13/bandwidthmon"
//! documentation = "https://!docs.rs/bandwidthmon"
//! keywords = [
//!     "network",
//!     "bandwidth",
//!     "monitor",
//!     "terminal",
//!     "cli",
//! ]
//! categories = [
//!     "command-line-utilities",
//!     "network-programming",
//! ]
//!
//! ── WHAT WAS BROKEN AND WHY ──────────────────────────────────────────────────
//!
//! BUG 1 — crossterm::terminal::size() returns garbage on Alpine Linux, inside
//!          tmux/screen, and over many SSH sessions. The old code used this as
//!          the sole source of terminal width. pingmon.rs uses term_size crate
//!          instead, which calls ioctl(TIOCGWINSZ) more reliably.
//!          FIX: switch to term_size::dimensions(), hard floor/ceiling on result.
//!
//! BUG 2 — chart_width was never clamped to data.len().
//!          rasciichart RIGHT-ALIGNS data inside the canvas. With 10 data points
//!          and a 200-column canvas, rasciichart emits 190 leading spaces per
//!          line — exactly what the screenshot showed ("725498.4│ ╭╮" with ~125
//!          leading spaces). This is not a rasciichart bug; it is expected
//!          behaviour. We must pass len(data) as the width, not the terminal width.
//!          FIX: effective_width = min(desired_width, data.len()).
//!
//! BUG 3 — rasciichart's with_width(W) is the PLOT area width. The library
//!          then prepends Y-axis labels (~10–12 chars wide) making the total
//!          line longer than the terminal → horizontal wrap / garbled output.
//!          FIX: reserve LABEL_RESERVE chars (12) so total line fits the terminal.
//!
//! BUG 4 — EnterAlternateScreen + resize_with(term_height, String::new) padded
//!          every frame with dozens of bare newlines, scrolling the screen.
//!          FIX: drop alternate screen entirely; use cursor_home + clear_to_eol
//!          per line + clear_to_eos at end, identical to pingmon.rs.
//!
//! ─────────────────────────────────────────────────────────────────────────────

use anyhow::{Context, Result};
use clap::{Parser, ArgAction};
use rasciichart::{plot_with_config, Config};
use std::collections::VecDeque;
use std::fmt;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::Networks;

// crossterm is used ONLY for raw-mode keyboard input — NOT for rendering.
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};

// ── Constants ─────────────────────────────────────────────────────────────────

const INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_HISTORY: usize = 120;
const DEFAULT_HEIGHT: usize = 10;

/// Chars reserved for rasciichart's Y-axis label column (e.g. "725498.4│ ").
const LABEL_RESERVE: usize = 12;

/// Never pass a plot width smaller than this to rasciichart.
const MIN_PLOT_WIDTH: usize = 8;

/// Hard cap — guards against ioctl returning an absurd value.
const MAX_PLOT_WIDTH: usize = 400;

// ── ANSI helpers (same pattern as pingmon.rs) ─────────────────────────────────

#[inline] fn cursor_home()  { print!("\x1B[H");       }  // move cursor to top-left
#[inline] fn clear_to_eol() { print!("\x1B[K");       }  // erase rest of current line
#[inline] fn clear_to_eos() { print!("\x1B[J");       }  // erase rest of screen
#[inline] fn clear_screen() { print!("\x1B[2J\x1B[H");}  // full clear (startup only)
#[inline] fn flush()        { let _ = stdout().flush(); } // one flush per frame

// ── Colour helpers ────────────────────────────────────────────────────────────

fn styled(text: &str, col: u8, bold: bool) -> String {
    if bold {
        format!("\x1b[1m\x1b[38;5;{}m{}\x1b[0m", col, text)
    } else {
        format!("\x1b[38;5;{}m{}\x1b[0m", col, text)
    }
}

const C_CYAN:   u8 = 51;
const C_YELLOW: u8 = 226;
const C_WHITE:  u8 = 15;
const C_GREY:   u8 = 240;
const C_GREEN:  u8 = 46;

// ── Terminal width (BUG 1 fix) ────────────────────────────────────────────────

/// Read terminal width via term_size (same crate as pingmon.rs).
/// Falls back to 80 on failure. Never returns 0, never panics.
fn term_cols() -> usize {
    term_size::dimensions()
        .map(|(w, _)| w)
        .unwrap_or(80)
        .max(40)   // sanity floor
        .min(500)  // sanity ceiling
}

// ── Version string ────────────────────────────────────────────────────────────

struct ColoredVersion;
impl fmt::Display for ColoredVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} by {}",
            styled("bandwidthmon", C_YELLOW, true),
            styled(env!("CARGO_PKG_VERSION"), C_WHITE, true),
            styled("Hadi Cahyadi <cumulus13@gmail.com>", C_CYAN, true),
        )
    }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    about = "Real-time network bandwidth monitor with rasciichart",
    disable_version_flag = true
)]
struct Args {
    /// Network interface to monitor (auto-select if not specified)
    #[arg(short, long)]
    iface: Option<String>,

    /// Chart height in lines
    #[arg(short = 'H', long, default_value_t = DEFAULT_HEIGHT)]
    height: usize,

    /// Chart width in columns (0 = auto-fit terminal)
    #[arg(short = 'W', long, default_value_t = 0)]
    width: usize,

    /// List available network interfaces and exit
    #[arg(short, long)]
    list: bool,

    /// Show peak / average / total summary
    #[arg(short, long)]
    summary: bool,

    /// Show download chart only
    #[arg(short, long)]
    download: bool,

    /// Show upload chart only
    #[arg(short, long)]
    upload: bool,

    /// Maximum number of history samples to keep
    #[arg(long, default_value_t = DEFAULT_HISTORY)]
    history: usize,

    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue)]
    version: bool,
}

// ── Bandwidth stats ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct BandwidthStats {
    download_bps: f64,
    upload_bps:   f64,
    total_rx:     u64,
    total_tx:     u64,
}

// ── Network monitor ───────────────────────────────────────────────────────────

struct NetworkMonitor {
    interface:    String,
    networks:     Networks,
    history_dl:   VecDeque<f64>,
    history_ul:   VecDeque<f64>,
    history_size: usize,           // explicit cap — don't rely on capacity()
    prev_rx:      u64,
    prev_tx:      u64,
    prev_time:    Instant,
    start_time:   Instant,
    peak_dl:      f64,
    peak_ul:      f64,
    avg_dl:       f64,
    avg_ul:       f64,
    sample_count: u64,
}

impl NetworkMonitor {
    fn new(interface: String, history_size: usize) -> Result<Self> {
        let networks = Networks::new_with_refreshed_list();
        if !networks.iter().any(|(n, _)| n == &interface) {
            anyhow::bail!("Interface '{}' not found", interface);
        }
        let (prev_rx, prev_tx) = networks.get(&interface)
            .map(|d| (d.total_received(), d.total_transmitted()))
            .unwrap_or((0, 0));
        let now = Instant::now();
        Ok(Self {
            interface, networks,
            history_dl:   VecDeque::with_capacity(history_size + 1),
            history_ul:   VecDeque::with_capacity(history_size + 1),
            history_size,
            prev_rx, prev_tx,
            prev_time: now, start_time: now,
            peak_dl: 0.0, peak_ul: 0.0,
            avg_dl:  0.0, avg_ul:  0.0,
            sample_count: 0,
        })
    }

    fn update(&mut self) -> Result<BandwidthStats> {
        self.networks.refresh(false);
        let data = self.networks.get(&self.interface)
            .context("Interface disappeared")?;

        let cur_rx   = data.total_received();
        let cur_tx   = data.total_transmitted();
        let cur_time = Instant::now();
        let elapsed  = cur_time.duration_since(self.prev_time).as_secs_f64();

        if elapsed < 0.001 {
            return Ok(BandwidthStats {
                download_bps: 0.0, upload_bps: 0.0,
                total_rx: cur_rx,  total_tx:  cur_tx,
            });
        }

        let dl_bps = cur_rx.saturating_sub(self.prev_rx) as f64 / elapsed;
        let ul_bps = cur_tx.saturating_sub(self.prev_tx) as f64 / elapsed;

        self.prev_rx   = cur_rx;
        self.prev_tx   = cur_tx;
        self.prev_time = cur_time;

        // History: compare against stored history_size (not .capacity()).
        if self.history_dl.len() >= self.history_size { self.history_dl.pop_front(); }
        self.history_dl.push_back(dl_bps);
        if self.history_ul.len() >= self.history_size { self.history_ul.pop_front(); }
        self.history_ul.push_back(ul_bps);

        // Welford online mean + running peak.
        self.peak_dl      = self.peak_dl.max(dl_bps);
        self.peak_ul      = self.peak_ul.max(ul_bps);
        self.sample_count += 1;
        self.avg_dl       += (dl_bps - self.avg_dl) / self.sample_count as f64;
        self.avg_ul       += (ul_bps - self.avg_ul) / self.sample_count as f64;

        Ok(BandwidthStats {
            download_bps: dl_bps, upload_bps: ul_bps,
            total_rx: cur_rx,     total_tx:  cur_tx,
        })
    }

    fn dl_history(&self) -> Vec<f64> { self.history_dl.iter().copied().collect() }
    fn ul_history(&self) -> Vec<f64> { self.history_ul.iter().copied().collect() }
}

// ── Formatting ────────────────────────────────────────────────────────────────

fn fmt_bps(b: f64) -> String {
    const U: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s"];
    let (mut v, mut i) = (b, 0usize);
    while v >= 1024.0 && i < U.len() - 1 { v /= 1024.0; i += 1; }
    format!("{:>7.2} {}", v, U[i])
}

fn fmt_total(b: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0usize);
    while v >= 1024.0 && i < U.len() - 1 { v /= 1024.0; i += 1; }
    format!("{:.2} {}", v, U[i])
}

// ── Interface helpers ─────────────────────────────────────────────────────────

fn list_interfaces() -> Result<()> {
    let nets = Networks::new_with_refreshed_list();
    println!("\n{}", styled("Available Network Interfaces:", C_CYAN, true));
    println!("{}", "─".repeat(60));
    for (name, data) in nets.iter() {
        println!("  {} {}",
            styled(name, C_WHITE, true),
            styled(&format!("(RX: {} bytes, TX: {} bytes)",
                data.total_received(), data.total_transmitted()), C_GREY, false));
    }
    println!();
    Ok(())
}

fn select_best_interface() -> Result<String> {
    Networks::new_with_refreshed_list()
        .iter()
        .max_by_key(|(_, d)| d.total_received() + d.total_transmitted())
        .map(|(n, _)| n.clone())
        .context("No network interfaces found")
}

fn resolve_interface(pattern: &str) -> Result<String> {
    let nets = Networks::new_with_refreshed_list();
    let all: Vec<String> = nets.iter().map(|(n, _)| n.clone()).collect();
    if all.iter().any(|n| n == pattern) { return Ok(pattern.to_string()); }
    let low = pattern.to_lowercase();
    let mut matches: Vec<String> = all.iter()
        .filter(|n| n.to_lowercase().contains(&low))
        .cloned().collect();
    if matches.is_empty() {
        anyhow::bail!("No interface matches '{}'. Available:\n{}", pattern, all.join("\n"));
    }
    matches.sort_by_key(|s| s.len());
    Ok(matches.remove(0))
}

// ── Safe plot width (BUG 2 + BUG 3 fix) ──────────────────────────────────────

/// Compute the plot width to pass to rasciichart, applying three constraints:
///
/// 1. Terminal-based limit: terminal_cols − LABEL_RESERVE.
///    (BUG 3) rasciichart prepends Y-axis labels; we must leave room for them.
///
/// 2. Data-length limit: never wider than the number of available samples.
///    (BUG 2) rasciichart right-aligns data in the canvas; with fewer points
///    than canvas columns you get huge leading whitespace per line.
///
/// 3. Hard floor (MIN_PLOT_WIDTH) and ceiling (MAX_PLOT_WIDTH).
///    (BUG 1 guard) protects against ioctl returning nonsense values.
fn safe_plot_width(user_requested: usize, data_len: usize, term_cols: usize) -> usize {
    // User can override terminal width with -W; 0 means "auto".
    let terminal_budget = if user_requested > 0 {
        user_requested
    } else {
        term_cols.saturating_sub(LABEL_RESERVE)
    };

    terminal_budget
        .min(data_len)       // never wider than available data points
        .min(MAX_PLOT_WIDTH)
        .max(MIN_PLOT_WIDTH)
}

// ── Chart renderer ────────────────────────────────────────────────────────────

/// Print a single chart to stdout using the pingmon.rs streaming pattern:
///   label line → chart lines with clear_to_eol() after each.
/// The very last chart line does NOT emit a newline; the caller decides.
fn print_chart(data: &[f64], height: usize, plot_width: usize, col: u8, label: &str) {
    if data.is_empty() || height == 0 || plot_width == 0 { return; }

    // Take only the most recent plot_width samples.
    let slice = {
        let start = data.len().saturating_sub(plot_width);
        &data[start..]
    };
    if slice.is_empty() { return; }

    // KEY: pass slice.len() as width, NOT plot_width.
    // Passing plot_width when slice.len() < plot_width causes leading whitespace.
    let config = Config::default()
        .with_height(height)
        .with_width(slice.len())
        .with_labels(true)
        .with_label_format("{:.1}".to_string());

    // Label header line.
    print!("{}", styled(label, col, true));
    clear_to_eol();
    println!();

    match plot_with_config(slice, config) {
        Err(e) => {
            print!("{}", styled(&format!("Chart error: {}", e), C_WHITE, false));
            clear_to_eol();
        }
        Ok(chart) => {
            let lines: Vec<&str> = chart.lines().collect();
            let last_idx = lines.len().saturating_sub(1);
            for (i, line) in lines.iter().enumerate() {
                print!("\x1b[38;5;{}m{}\x1b[0m", col, line);
                clear_to_eol();
                if i < last_idx { println!(); }  // no newline on very last line
            }
        }
    }
}

// ── Frame renderer ────────────────────────────────────────────────────────────

/// Render one complete UI frame.
/// Uses the pingmon.rs pattern: cursor_home → lines with clear_to_eol
/// → clear_to_eos → single flush. No alternate screen, no String padding.
fn render_frame(monitor: &NetworkMonitor, stats: &BandwidthStats, args: &Args) {
    // BUG 1 fix: read width fresh every frame via term_size, not crossterm.
    let tw = term_cols();

    cursor_home();

    // ── Header ──────────────────────────────────────────────────────────────
    print!("{}", styled(
        &format!("═══ Bandwidth Monitor ({}) ═══", monitor.interface),
        C_CYAN, true));
    clear_to_eol(); println!();

    // ── Current speeds ───────────────────────────────────────────────────────
    print!("{} {}  │  {} {}  {}",
        styled("Download:", C_CYAN,   true), styled(&fmt_bps(stats.download_bps), C_WHITE, false),
        styled("Upload:",   C_YELLOW, true), styled(&fmt_bps(stats.upload_bps),   C_WHITE, false),
        styled("'q'/Ctrl-C to quit", C_GREY, false));
    clear_to_eol(); println!();

    // ── Summary (optional) ───────────────────────────────────────────────────
    if args.summary {
        print!("{} {}  │  {} {}",
            styled("Peak DL:", C_CYAN,   false), styled(&fmt_bps(monitor.peak_dl), C_WHITE, false),
            styled("Peak UL:", C_YELLOW, false), styled(&fmt_bps(monitor.peak_ul), C_WHITE, false));
        clear_to_eol(); println!();

        print!("{} {}  │  {} {}",
            styled("Avg DL:", C_CYAN,   false), styled(&fmt_bps(monitor.avg_dl), C_WHITE, false),
            styled("Avg UL:", C_YELLOW, false), styled(&fmt_bps(monitor.avg_ul), C_WHITE, false));
        clear_to_eol(); println!();

        print!("{} {}  │  {} {}",
            styled("Total RX:", C_CYAN,   false), styled(&fmt_total(stats.total_rx), C_WHITE, false),
            styled("Total TX:", C_YELLOW, false), styled(&fmt_total(stats.total_tx), C_WHITE, false));
        clear_to_eol(); println!();

        print!("{} {:.1}s",
            styled("Runtime:", C_GREEN, false),
            monitor.start_time.elapsed().as_secs_f64());
        clear_to_eol(); println!();
    }

    // Blank separator.
    clear_to_eol(); println!();

    let show_both = !args.download && !args.upload;

    // ── Download chart ───────────────────────────────────────────────────────
    if args.download || show_both {
        let dl = monitor.dl_history();
        if !dl.is_empty() {
            let pw = safe_plot_width(args.width, dl.len(), tw);
            print_chart(&dl, args.height, pw, C_CYAN, "▼ Download Speed");
            println!(); clear_to_eol(); println!();
        }
    }

    // ── Upload chart ─────────────────────────────────────────────────────────
    if args.upload || show_both {
        let ul = monitor.ul_history();
        if !ul.is_empty() {
            let pw = safe_plot_width(args.width, ul.len(), tw);
            print_chart(&ul, args.height, pw, C_YELLOW, "▲ Upload Speed");
            // No trailing println — clear_to_eos erases leftover screen below.
        }
    }

    // BUG 4 fix: erase everything below the last drawn line, then flush once.
    clear_to_eos();
    flush();
}

// ── Monitor loop ──────────────────────────────────────────────────────────────

fn monitor_bandwidth(args: Args) -> Result<()> {
    let interface = if let Some(ref iface) = args.iface {
        resolve_interface(iface)?
    } else {
        select_best_interface()?
    };

    println!("Monitoring interface: {}\n", styled(&interface, C_CYAN, true));

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || { r.store(false, Ordering::SeqCst); })?;

    let mut monitor = NetworkMonitor::new(interface, args.history)?;

    // Warm-up: discard first sample — elapsed includes init time → fake spike.
    let _ = monitor.update();
    let mut last_update = Instant::now();

    // Clear once at startup (no alternate screen — matches pingmon.rs).
    clear_screen();
    flush();

    // Raw mode only for keyboard reading.
    enable_raw_mode()?;

    let result: Result<()> = (|| {
        loop {
            if !running.load(Ordering::SeqCst) { break; }

            // Non-blocking poll — 50 ms keeps the UI snappy without busy-spin.
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(k) = event::read()? {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                        _ => {}
                    }
                }
            }

            if last_update.elapsed() >= INTERVAL {
                let stats = monitor.update()?;
                render_frame(&monitor, &stats, &args);
                last_update = Instant::now();
            }
        }
        Ok(())
    })();

    // Restore terminal — no ? so it always runs, even after an error.
    let _ = disable_raw_mode();

    // Print exit message below the last render.
    println!("\n\n{}", styled("Stopped cleanly.", C_GREEN, true));

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();
    if args.version { println!("{}", ColoredVersion); return Ok(()); }
    if args.list    { return list_interfaces(); }
    monitor_bandwidth(args)
}