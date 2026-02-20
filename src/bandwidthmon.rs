// File: src/bandwidthmon.rs
//! Bandwidth Monitor - Using custom rasciichart with auto-resize
//! Author: Hadi Cahyadi <cumulus13@gmail.com>
//! License: MIT

use anyhow::{Context, Result};
use clap::{Parser, ArgAction};
use rasciichart::{plot_with_config, Config};
use std::collections::VecDeque;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fmt;
use sysinfo::Networks;

// ── REMOVED: crossterm entirely for rendering.
// crossterm's EnterAlternateScreen + MoveTo(0,0) + Print(full_output) is the
// primary cause of "chaos" on Linux/macOS terminals. The alternate screen
// buffer behaves inconsistently across terminal emulators (gnome-terminal,
// kitty, iTerm2, tmux, screen, etc.) when:
//   1. You move to (0,0) but the previous render left more lines than the
//      current one — stale lines remain visible below.
//   2. resize_with(term_height, String::new) pads with empty Strings but
//      those become bare newlines that scroll the terminal.
//   3. Print(full_output) writes ANSI escape codes that are not flushed
//      atomically, causing tearing on slow terminals / piped output.
//   4. enable_raw_mode() on Linux intercepts ALL signals differently from
//      macOS, causing Ctrl-C to leave the terminal in raw mode on panic.
//
// pingmon.rs works correctly because it uses:
//   • \x1B[H  (cursor home, no alternate screen)
//   • \x1B[K  (erase to end of line, per-line — no full-screen redraw)
//   • \x1B[J  (erase rest of screen — cleans up without blank-line padding)
//   • direct print!/println! — no crossterm abstraction layer
//   • manual flush only once per frame via io::stdout().flush()
//
// We replicate that exact approach here and drop crossterm for rendering.
// crossterm is still used ONLY for reading terminal size (size()) and
// keyboard events (event::poll / event::read) because those are genuinely
// cross-platform utilities. Raw mode is now entered/exited safely.

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, size},
};

const INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_HISTORY: usize = 120;
const DEFAULT_HEIGHT: usize = 10;

// ── ANSI helpers (mirrors pingmon.rs style) ──────────────────────────────────

/// Move cursor to top-left without switching to alternate screen.
#[inline]
fn cursor_home() {
    print!("\x1B[H");
}

/// Erase from cursor to end of current line.
#[inline]
fn clear_to_eol() {
    print!("\x1B[K");
}

/// Erase from cursor to end of screen (clears leftover lines from previous
/// taller render without inserting blank lines).
#[inline]
fn clear_to_eos() {
    print!("\x1B[J");
}

/// Clear entire screen and home cursor (used once at startup).
#[inline]
fn clear_screen() {
    print!("\x1B[2J\x1B[H");
}

/// Flush stdout — called once per frame, not per line.
#[inline]
fn flush() {
    let _ = stdout().flush();
}

// ── Colour helpers ────────────────────────────────────────────────────────────

fn style_text(text: &str, color_code: u8, bold: bool) -> String {
    if bold {
        format!("\x1b[1m\x1b[38;5;{}m{}\x1b[0m", color_code, text)
    } else {
        format!("\x1b[38;5;{}m{}\x1b[0m", color_code, text)
    }
}

// Named colour constants — avoids the crossterm Color enum dependency in
// rendering code.
const COL_CYAN: u8 = 51;
const COL_YELLOW: u8 = 226;
const COL_WHITE: u8 = 15;
const COL_GREY: u8 = 240;
const COL_GREEN: u8 = 46;

// ── Version display ───────────────────────────────────────────────────────────

struct ColoredVersion;

impl fmt::Display for ColoredVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name    = style_text("bandwidthmon", COL_YELLOW, true);
        let version = style_text(env!("CARGO_PKG_VERSION"), COL_WHITE, true);
        let author  = style_text("Hadi Cahyadi <cumulus13@gmail.com>", COL_CYAN, true);
        write!(f, "{} {} by {}", name, version, author)
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

    /// Chart width in columns (auto-fit terminal if 0)
    #[arg(short = 'W', long, default_value_t = 0)]
    width: usize,

    /// List available network interfaces
    #[arg(short, long)]
    list: bool,

    /// Show summary statistics
    #[arg(short, long)]
    summary: bool,

    /// Show download chart only
    #[arg(short, long)]
    download: bool,

    /// Show upload chart only
    #[arg(short, long)]
    upload: bool,

    /// Maximum history points
    #[arg(long, default_value_t = DEFAULT_HISTORY)]
    history: usize,

    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue)]
    version: bool,
}

// ── Bandwidth stats ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct BandwidthStats {
    download_bps: f64,
    upload_bps: f64,
    total_rx: u64,
    total_tx: u64,
}

// ── Network monitor ───────────────────────────────────────────────────────────

struct NetworkMonitor {
    interface: String,
    networks: Networks,
    history_dl: VecDeque<f64>,
    history_ul: VecDeque<f64>,
    prev_rx: u64,
    prev_tx: u64,
    prev_time: Instant,
    start_time: Instant,
    peak_dl: f64,
    peak_ul: f64,
    avg_dl: f64,
    avg_ul: f64,
    sample_count: u64,
    history_size: usize,
}

impl NetworkMonitor {
    fn new(interface: String, history_size: usize) -> Result<Self> {
        let networks = Networks::new_with_refreshed_list();

        if !networks.iter().any(|(name, _)| name == &interface) {
            anyhow::bail!("Interface '{}' not found", interface);
        }

        let (prev_rx, prev_tx) = networks
            .get(&interface)
            .map(|d| (d.total_received(), d.total_transmitted()))
            .unwrap_or((0, 0));

        let now = Instant::now();

        Ok(Self {
            interface,
            networks,
            history_dl: VecDeque::with_capacity(history_size + 1),
            history_ul: VecDeque::with_capacity(history_size + 1),
            prev_rx,
            prev_tx,
            prev_time: now,
            start_time: now,
            peak_dl: 0.0,
            peak_ul: 0.0,
            avg_dl: 0.0,
            avg_ul: 0.0,
            sample_count: 0,
            history_size,
        })
    }

    fn update(&mut self) -> Result<BandwidthStats> {
        self.networks.refresh(false);

        let data = self
            .networks
            .get(&self.interface)
            .context("Interface disappeared")?;

        let cur_rx = data.total_received();
        let cur_tx = data.total_transmitted();
        let cur_time = Instant::now();

        let elapsed = cur_time.duration_since(self.prev_time).as_secs_f64();

        if elapsed < 0.001 {
            return Ok(BandwidthStats {
                download_bps: 0.0,
                upload_bps: 0.0,
                total_rx: cur_rx,
                total_tx: cur_tx,
            });
        }

        let dl_bytes = cur_rx.saturating_sub(self.prev_rx);
        let ul_bytes = cur_tx.saturating_sub(self.prev_tx);
        let dl_bps = dl_bytes as f64 / elapsed;
        let ul_bps = ul_bytes as f64 / elapsed;

        self.prev_rx = cur_rx;
        self.prev_tx = cur_tx;
        self.prev_time = cur_time;

        // Maintain capped history (pop before push to avoid over-alloc).
        if self.history_dl.len() >= self.history_size {
            self.history_dl.pop_front();
        }
        self.history_dl.push_back(dl_bps);

        if self.history_ul.len() >= self.history_size {
            self.history_ul.pop_front();
        }
        self.history_ul.push_back(ul_bps);

        // Running peak & Welford online average.
        self.peak_dl = self.peak_dl.max(dl_bps);
        self.peak_ul = self.peak_ul.max(ul_bps);
        self.sample_count += 1;
        self.avg_dl += (dl_bps - self.avg_dl) / self.sample_count as f64;
        self.avg_ul += (ul_bps - self.avg_ul) / self.sample_count as f64;

        Ok(BandwidthStats {
            download_bps: dl_bps,
            upload_bps: ul_bps,
            total_rx: cur_rx,
            total_tx: cur_tx,
        })
    }

    fn history_dl(&self) -> Vec<f64> { self.history_dl.iter().copied().collect() }
    fn history_ul(&self) -> Vec<f64> { self.history_ul.iter().copied().collect() }
}

// ── Formatting ────────────────────────────────────────────────────────────────

fn format_bps(bytes: f64) -> String {
    const UNITS: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = bytes;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 { v /= 1024.0; i += 1; }
    format!("{:>7.2} {}", v, UNITS[i])
}

fn format_total(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 { v /= 1024.0; i += 1; }
    format!("{:.2} {}", v, UNITS[i])
}

// ── Interface helpers ─────────────────────────────────────────────────────────

fn list_interfaces() -> Result<()> {
    let networks = Networks::new_with_refreshed_list();
    println!("\n{}", style_text("Available Network Interfaces:", COL_CYAN, true));
    println!("{}", "─".repeat(60));
    for (name, data) in networks.iter() {
        println!(
            "  {} {}",
            style_text(name, COL_WHITE, true),
            style_text(
                &format!("(RX: {} bytes, TX: {} bytes)",
                    data.total_received(), data.total_transmitted()),
                COL_GREY, false
            )
        );
    }
    println!();
    Ok(())
}

fn select_best_interface() -> Result<String> {
    Networks::new_with_refreshed_list()
        .iter()
        .max_by_key(|(_, d)| d.total_received() + d.total_transmitted())
        .map(|(name, _)| name.clone())
        .context("No network interfaces found")
}

fn resolve_interface(pattern: &str) -> Result<String> {
    let networks = Networks::new_with_refreshed_list();
    let all: Vec<String> = networks.iter().map(|(n, _)| n.clone()).collect();

    // 1. Exact match.
    if all.iter().any(|n| n == pattern) {
        return Ok(pattern.to_string());
    }

    // 2. Case-insensitive partial match.
    let low = pattern.to_lowercase();
    let mut matches: Vec<String> = all.iter()
        .filter(|n| n.to_lowercase().contains(&low))
        .cloned()
        .collect();

    if matches.is_empty() {
        anyhow::bail!(
            "No interface matches '{}'. Available:\n{}", pattern, all.join("\n")
        );
    }

    matches.sort_by_key(|s| s.len());
    Ok(matches.remove(0))
}

// ── Chart rendering ───────────────────────────────────────────────────────────

/// Render one chart, writing directly to stdout line-by-line.
/// Each line is followed by \x1B[K (erase to EOL) to prevent stale chars
/// when the terminal is wider than the chart.  No String allocation for the
/// full frame — we stream line-by-line exactly like pingmon.rs does.
fn print_chart(data: &[f64], height: usize, width: usize, color: u8, label: &str) {
    if data.is_empty() || height == 0 || width == 0 {
        return;
    }

    // Use the most recent `width` samples.
    let start = data.len().saturating_sub(width);
    let slice = &data[start..];
    if slice.is_empty() { return; }

    let config = Config::default()
        .with_height(height)
        .with_width(width)
        .with_labels(true)
        .with_label_format("{:.1}".to_string());

    match plot_with_config(slice, config) {
        Err(e) => {
            print!("{}", style_text(&format!("Chart error: {}", e), COL_WHITE, false));
            clear_to_eol();
            println!();
        }
        Ok(chart) => {
            // Label line.
            print!("{}", style_text(label, color, true));
            clear_to_eol();
            println!();

            let lines: Vec<&str> = chart.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                print!("\x1b[38;5;{}m{}\x1b[0m", color, line);
                clear_to_eol();
                // Avoid trailing newline on last line — \x1B[J after the
                // last chart already advances the cursor correctly.
                if i < lines.len() - 1 {
                    println!();
                }
            }
        }
    }
}

// ── Main render frame ─────────────────────────────────────────────────────────

/// Render one complete UI frame.  Uses the pingmon.rs technique:
///   cursor_home → print lines with clear_to_eol → clear_to_eos → flush.
/// This avoids alternate-screen issues and eliminates tearing.
fn render_frame(monitor: &NetworkMonitor, stats: &BandwidthStats, args: &Args, term_width: u16) {
    let chart_width = if args.width > 0 {
        args.width
    } else {
        // Reserve ~12 chars for rasciichart's Y-axis labels and a small margin.
        (term_width as usize).saturating_sub(14).max(20)
    };

    cursor_home();

    // ── Header ──
    print!("{}", style_text(
        &format!("═══ Bandwidth Monitor ({}) ═══", monitor.interface),
        COL_CYAN, true,
    ));
    clear_to_eol();
    println!();

    // ── Current speeds ──
    print!("{} {}  │  {} {}  {}",
        style_text("Download:", COL_CYAN, true),
        style_text(&format_bps(stats.download_bps), COL_WHITE, false),
        style_text("Upload:",   COL_YELLOW, true),
        style_text(&format_bps(stats.upload_bps),   COL_WHITE, false),
        style_text("Press 'q' or Ctrl+C to quit",   COL_GREY,  false),
    );
    clear_to_eol();
    println!();

    // ── Optional summary ──
    if args.summary {
        print!("{} {}  │  {} {}",
            style_text("Peak DL:", COL_CYAN, false),
            style_text(&format_bps(monitor.peak_dl), COL_WHITE, false),
            style_text("Peak UL:", COL_YELLOW, false),
            style_text(&format_bps(monitor.peak_ul), COL_WHITE, false),
        );
        clear_to_eol();
        println!();

        print!("{} {}  │  {} {}",
            style_text("Avg DL:", COL_CYAN, false),
            style_text(&format_bps(monitor.avg_dl), COL_WHITE, false),
            style_text("Avg UL:", COL_YELLOW, false),
            style_text(&format_bps(monitor.avg_ul), COL_WHITE, false),
        );
        clear_to_eol();
        println!();

        print!("{} {}  │  {} {}",
            style_text("Total RX:", COL_CYAN, false),
            style_text(&format_total(stats.total_rx), COL_WHITE, false),
            style_text("Total TX:", COL_YELLOW, false),
            style_text(&format_total(stats.total_tx), COL_WHITE, false),
        );
        clear_to_eol();
        println!();

        print!("{} {:.1}s",
            style_text("Runtime:", COL_GREEN, false),
            monitor.start_time.elapsed().as_secs_f64(),
        );
        clear_to_eol();
        println!();
    }

    // Blank separator line.
    clear_to_eol();
    println!();

    let show_both = !args.download && !args.upload;

    // ── Download chart ──
    if args.download || show_both {
        let dl = monitor.history_dl();
        if !dl.is_empty() {
            print_chart(&dl, args.height, chart_width, COL_CYAN, "▼ Download Speed");
            println!();
            clear_to_eol();
            println!();
        }
    }

    // ── Upload chart ──
    if args.upload || show_both {
        let ul = monitor.history_ul();
        if !ul.is_empty() {
            print_chart(&ul, args.height, chart_width, COL_YELLOW, "▲ Upload Speed");
            // No trailing newline — clear_to_eos handles the rest.
        }
    }

    // Erase everything below the last printed line.  This is the key
    // technique that replaces the broken resize_with(term_height, …) padding.
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

    println!("Monitoring interface: {}\n",
        style_text(&interface, COL_CYAN, true));

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Ctrl-C handler — disables raw mode before exiting so the terminal is
    // never left in a broken state.
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    // Clear screen once — no alternate screen buffer.
    clear_screen();
    flush();

    // Enter raw mode only for keyboard input; NOT for rendering.
    // This is why pingmon.rs works: it never calls enable_raw_mode() because
    // it only needs Ctrl-C (handled by ctrlc crate) and 'q'.  We enter raw
    // mode here only to catch 'q' and Esc, and we wrap everything in a
    // cleanup guard so raw mode is always disabled on exit.
    enable_raw_mode()?;

    let mut monitor = NetworkMonitor::new(interface, args.history)?;
    let mut last_update = Instant::now();

    // Warm-up: first sample sets baseline counters; discard it.
    let _ = monitor.update();
    last_update = Instant::now();

    let result: Result<()> = (|| {
        loop {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            // Non-blocking key poll (50 ms window — keeps UI responsive).
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        _ => {}
                    }
                }
            }

            if last_update.elapsed() >= INTERVAL {
                let stats = monitor.update()?;

                // Re-read terminal size each frame — handles live resizing.
                let (term_width, _) = size()?;

                render_frame(&monitor, &stats, &args, term_width);
                last_update = Instant::now();
            }
        }
        Ok(())
    })();

    // ── Always restore terminal ───────────────────────────────────────────────
    // Disable raw mode unconditionally.  We intentionally do NOT use an
    // alternate screen, so there is nothing to "leave" — the terminal
    // naturally returns to the normal scroll buffer on process exit.
    let _ = disable_raw_mode();

    // Print a clean exit message below the last render.
    println!("\n{}", style_text("Stopped cleanly.", COL_GREEN, true));

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();

    if args.version {
        println!("{}", ColoredVersion);
        return Ok(());
    }

    if args.list {
        return list_interfaces();
    }

    monitor_bandwidth(args)
}