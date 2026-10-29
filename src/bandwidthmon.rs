// File: src/bandwidthmon.rs
//! Bandwidth Monitor - robust, production-ready, cross-platform
//! Author: Hadi Cahyadi <cumulus13@gmail.com>
//! License: MIT
//!
//! ── HISTORY OF THE "CHAOS" BUGS ──────────────────────────────────────────────
//!
//! BUG 1 — crossterm::terminal::size() can return garbage (Alpine, tmux/screen,
//!          some SSH sessions). FIX: term_size (ioctl TIOCGWINSZ) first, with
//!          env/default fallbacks and a sane clamp  -> term_ui::term_size().
//! BUG 2 — rasciichart right-aligns data inside its canvas; width must be
//!          min(budget, data.len()) or every line gets leading spaces.
//! BUG 3 — with_width(W) is the PLOT width; the Y-axis labels come on top, so
//!          the label width must be reserved or the line wraps.
//! BUG 4 — padding the frame with bare newlines scrolled the screen.
//! BUG 5 — Unix raw mode clears OPOST, so a bare '\n' has no CR (staircase).
//!          FIX: frames are joined with "\r\n" in term_ui::Screen::draw().
//!
//! STILL BROKEN OVER SSH (fixed in this version, see term_ui.rs for details):
//! BUG 6 — frame taller than the terminal (24 rows) scrolled the screen.
//! BUG 7 — header line wider than the terminal wrapped and shifted the frame.
//! BUG 8 — ~11 small write()s per frame tore/flickered on slow links.
//! BUG 9 — `ssh host bandwidthmon` (no PTY) died with "No such device or
//!          address". Now falls back to plain one-line-per-second output.
//! BUG 10 — UTF-8 box glyphs / 256 colours were forced regardless of the
//!          locale and $TERM. Now auto-degrades (ASCII / 16 colours / none);
//!          `--ascii` forces it, NO_COLOR is honoured.
//! ─────────────────────────────────────────────────────────────────────────────

#[path = "term_ui.rs"]
mod term_ui;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser};
use clap_version_flag::colorful_version;
use clap_color_help::default_styles;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use rasciichart::{plot_with_config, Config};
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::Networks;
use term_ui::{styled, Screen};

// ── Constants ─────────────────────────────────────────────────────────────────

const INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_HISTORY: usize = 120;
const DEFAULT_HEIGHT: usize = 10;

/// Minimum chars reserved for rasciichart's Y-axis label column ("725498.4│ ").
const MIN_LABEL_RESERVE: usize = 12;
const MIN_PLOT_WIDTH: usize = 8;
const MAX_PLOT_WIDTH: usize = 400;

const C_CYAN: u8 = 51;
const C_YELLOW: u8 = 226;
const C_WHITE: u8 = 15;
const C_GREY: u8 = 240;
const C_GREEN: u8 = 46;

// ── Version string ────────────────────────────────────────────────────────────

struct ColoredVersion;
impl fmt::Display for ColoredVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} by {}",
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
    disable_version_flag = true,
    styles = default_styles()
)]
struct Args {
    /// Network interface to monitor (auto-select if not specified)
    #[arg(short, long)]
    iface: Option<String>,

    /// Chart height in lines (shrunk automatically to fit the terminal)
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

    /// Plain output: one text line per second (auto when there is no TTY)
    #[arg(long)]
    plain: bool,

    /// Force ASCII-only drawing (auto when the locale/TERM is not UTF-8)
    #[arg(long)]
    ascii: bool,

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
    history_size: usize,
    prev_rx: u64,
    prev_tx: u64,
    prev_time: Instant,
    start_time: Instant,
    peak_dl: f64,
    peak_ul: f64,
    avg_dl: f64,
    avg_ul: f64,
    sample_count: u64,
}

impl NetworkMonitor {
    fn new(interface: String, history_size: usize) -> Result<Self> {
        let networks = Networks::new_with_refreshed_list();
        if !networks.iter().any(|(n, _)| n == &interface) {
            anyhow::bail!("Interface '{}' not found", interface);
        }
        let (prev_rx, prev_tx) = networks
            .get(&interface)
            .map(|d| (d.total_received(), d.total_transmitted()))
            .unwrap_or((0, 0));
        let history_size = history_size.max(1);
        let now = Instant::now();
        Ok(Self {
            interface,
            networks,
            history_dl: VecDeque::with_capacity(history_size + 1),
            history_ul: VecDeque::with_capacity(history_size + 1),
            history_size,
            prev_rx,
            prev_tx,
            prev_time: now,
            start_time: now,
            peak_dl: 0.0,
            peak_ul: 0.0,
            avg_dl: 0.0,
            avg_ul: 0.0,
            sample_count: 0,
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

        let dl_bps = cur_rx.saturating_sub(self.prev_rx) as f64 / elapsed;
        let ul_bps = cur_tx.saturating_sub(self.prev_tx) as f64 / elapsed;

        self.prev_rx = cur_rx;
        self.prev_tx = cur_tx;
        self.prev_time = cur_time;

        if self.history_dl.len() >= self.history_size {
            self.history_dl.pop_front();
        }
        self.history_dl.push_back(dl_bps);
        if self.history_ul.len() >= self.history_size {
            self.history_ul.pop_front();
        }
        self.history_ul.push_back(ul_bps);

        // Welford online mean + running peak.
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

    fn dl_history(&self) -> Vec<f64> {
        self.history_dl.iter().copied().collect()
    }
    fn ul_history(&self) -> Vec<f64> {
        self.history_ul.iter().copied().collect()
    }
}

// ── Formatting ────────────────────────────────────────────────────────────────

fn fmt_bps(b: f64) -> String {
    const U: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s"];
    let (mut v, mut i) = (b, 0usize);
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:>7.2} {}", v, U[i])
}

fn fmt_total(b: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0usize);
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.2} {}", v, U[i])
}

// ── Interface helpers ─────────────────────────────────────────────────────────

fn list_interfaces() -> Result<()> {
    let nets = Networks::new_with_refreshed_list();
    println!("\n{}", styled("Available Network Interfaces:", C_CYAN, true));
    println!("{}", "─".repeat(60));
    for (name, data) in nets.iter() {
        println!(
            "  {} {}",
            styled(name, C_WHITE, true),
            styled(
                &format!(
                    "(RX: {} bytes, TX: {} bytes)",
                    data.total_received(),
                    data.total_transmitted()
                ),
                C_GREY,
                false
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
        .map(|(n, _)| n.clone())
        .context("No network interfaces found")
}

fn resolve_interface(pattern: &str) -> Result<String> {
    let nets = Networks::new_with_refreshed_list();
    let all: Vec<String> = nets.iter().map(|(n, _)| n.clone()).collect();
    if all.iter().any(|n| n == pattern) {
        return Ok(pattern.to_string());
    }
    let low = pattern.to_lowercase();
    let mut matches: Vec<String> = all
        .iter()
        .filter(|n| n.to_lowercase().contains(&low))
        .cloned()
        .collect();
    if matches.is_empty() {
        anyhow::bail!("No interface matches '{}'. Available:\n{}", pattern, all.join("\n"));
    }
    matches.sort_by_key(|s| s.len());
    Ok(matches.remove(0))
}

// ── Chart ─────────────────────────────────────────────────────────────────────

/// Plot width for rasciichart: terminal budget (minus the Y-axis label column),
/// never wider than the data (BUG 2), floor/ceiling for sanity.
fn safe_plot_width(user_requested: usize, data_len: usize, term_cols: usize, label_reserve: usize) -> usize {
    let budget = if user_requested > 0 {
        user_requested
    } else {
        term_cols.saturating_sub(label_reserve)
    };
    budget.min(data_len).min(MAX_PLOT_WIDTH).max(MIN_PLOT_WIDTH)
}

/// Label line + chart lines (joined by '\n', no trailing newline).
fn chart_block(data: &[f64], height: usize, term_cols: usize, user_width: usize, col: u8, label: &str) -> Option<String> {
    if data.is_empty() || height == 0 {
        return None;
    }
    // Y-axis label column = widest label ("{:.1}" of the max) + "│ ".
    let max = data.iter().copied().fold(0.0_f64, f64::max);
    let label_reserve = (format!("{:.1}", max).len() + 3).max(MIN_LABEL_RESERVE);

    let pw = safe_plot_width(user_width, data.len(), term_cols, label_reserve);
    let slice = &data[data.len().saturating_sub(pw)..];

    // BUG 2: pass slice.len(), not the terminal width.
    let config = Config::default()
        .with_height(height)
        .with_width(slice.len())
        .with_labels(true)
        .with_label_format("{:.1}".to_string());

    let mut out = styled(label, col, true);
    match plot_with_config(slice, config) {
        Err(e) => {
            out.push('\n');
            out.push_str(&format!("Chart error: {}", e));
        }
        Ok(chart) => {
            for line in chart.lines() {
                out.push('\n');
                out.push_str(&styled(line, col, false));
            }
        }
    }
    Some(out)
}

// ── Frame ─────────────────────────────────────────────────────────────────────

/// Whole UI as one string ('\n'-separated). Screen::draw() clips/draws it.
fn build_frame(monitor: &NetworkMonitor, stats: &BandwidthStats, args: &Args) -> String {
    let (cols, rows) = term_ui::term_size();
    let both = !args.download && !args.upload;
    let show_dl = args.download || both;
    let show_ul = args.upload || both;
    let charts = show_dl as usize + show_ul as usize;
    let (height, show_summary) = term_ui::fit_chart_height(rows, args.height, charts, args.summary, 2);

    let mut l: Vec<String> = Vec::new();

    l.push(styled(
        &format!("═══ Bandwidth Monitor ({}) ═══", monitor.interface),
        C_CYAN,
        true,
    ));

    let hint = if cols >= 78 {
        "'q'/Ctrl-C to quit"
    } else if cols >= 56 {
        "q=quit"
    } else {
        ""
    };
    l.push(format!(
        "{} {}  │  {} {}  {}",
        styled("Download:", C_CYAN, true),
        styled(&fmt_bps(stats.download_bps), C_WHITE, false),
        styled("Upload:", C_YELLOW, true),
        styled(&fmt_bps(stats.upload_bps), C_WHITE, false),
        styled(hint, C_GREY, false),
    ));

    if show_summary {
        l.push(format!(
            "{} {}  │  {} {}",
            styled("Peak DL:", C_CYAN, false),
            styled(&fmt_bps(monitor.peak_dl), C_WHITE, false),
            styled("Peak UL:", C_YELLOW, false),
            styled(&fmt_bps(monitor.peak_ul), C_WHITE, false),
        ));
        l.push(format!(
            "{} {}  │  {} {}",
            styled("Avg DL:", C_CYAN, false),
            styled(&fmt_bps(monitor.avg_dl), C_WHITE, false),
            styled("Avg UL:", C_YELLOW, false),
            styled(&fmt_bps(monitor.avg_ul), C_WHITE, false),
        ));
        l.push(format!(
            "{} {}  │  {} {}",
            styled("Total RX:", C_CYAN, false),
            styled(&fmt_total(stats.total_rx), C_WHITE, false),
            styled("Total TX:", C_YELLOW, false),
            styled(&fmt_total(stats.total_tx), C_WHITE, false),
        ));
        l.push(format!(
            "{} {:.1}s",
            styled("Runtime:", C_GREEN, false),
            monitor.start_time.elapsed().as_secs_f64()
        ));
    }

    l.push(String::new());

    if show_dl {
        if let Some(c) = chart_block(&monitor.dl_history(), height, cols, args.width, C_CYAN, "▼ Download Speed") {
            l.push(c);
        }
        if show_ul {
            l.push(String::new());
        }
    }
    if show_ul {
        if let Some(c) = chart_block(&monitor.ul_history(), height, cols, args.width, C_YELLOW, "▲ Upload Speed") {
            l.push(c);
        }
    }

    l.join("\n")
}

fn waiting_frame(iface: &str) -> String {
    format!(
        "{}\n{}",
        styled(&format!("═══ Bandwidth Monitor ({}) ═══", iface), C_CYAN, true),
        styled("collecting first sample…  (q to quit)", C_GREY, false)
    )
}

// ── Monitor loop ──────────────────────────────────────────────────────────────

fn run_loop(
    monitor: &mut NetworkMonitor,
    args: &Args,
    running: &AtomicBool,
    screen: &mut Option<Screen>,
) -> Result<()> {
    let mut last_update = Instant::now();
    let mut last_stats: Option<BandwidthStats> = None;
    let mut redraw = screen.is_some(); // paint something immediately

    while running.load(Ordering::SeqCst) {
        // ── input / idle ────────────────────────────────────────────────────
        if screen.is_some() {
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                        _ => {}
                    },
                    // ssh window resize: reflow right away instead of at the next tick
                    Event::Resize(_, _) => redraw = true,
                    _ => {}
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }

        // ── sample ──────────────────────────────────────────────────────────
        if last_update.elapsed() >= INTERVAL {
            let stats = monitor.update()?;
            last_update = Instant::now();

            match screen.as_mut() {
                Some(_) => {
                    last_stats = Some(stats);
                    redraw = true;
                }
                None => {
                    let line = term_ui::plain_line(
                        &monitor.interface,
                        monitor.start_time.elapsed().as_secs(),
                        &fmt_bps(stats.download_bps),
                        &fmt_bps(stats.upload_bps),
                    );
                    if !term_ui::emit_plain(&line) {
                        break; // stdout closed (e.g. `| head`)
                    }
                }
            }
        }

        // ── paint ───────────────────────────────────────────────────────────
        if redraw {
            if let Some(s) = screen.as_mut() {
                let frame = match &last_stats {
                    Some(st) => build_frame(monitor, st, args),
                    None => waiting_frame(&monitor.interface),
                };
                s.draw(&frame)?;
            }
            redraw = false;
        }
    }
    Ok(())
}

fn monitor_bandwidth(args: Args) -> Result<()> {
    let interface = if let Some(ref iface) = args.iface {
        resolve_interface(iface)?
    } else {
        select_best_interface()?
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    // "termination" feature: SIGTERM/SIGHUP (ssh drop) also stop cleanly.
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst))?;

    let mut monitor = NetworkMonitor::new(interface, args.history)?;

    // Warm-up: discard first sample — elapsed includes init time → fake spike.
    let _ = monitor.update();

    let mut screen = Screen::try_enter(args.plain);
    if screen.is_none() {
        eprintln!(
            "{} {}  (plain mode, Ctrl-C to stop)",
            styled("Monitoring interface:", C_GREEN, false),
            styled(&monitor.interface, C_CYAN, true)
        );
    }

    let result = run_loop(&mut monitor, &args, &running, &mut screen);

    // Restore the terminal BEFORE printing anything else.
    drop(screen);

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    eprintln!("{}", styled("Stopped cleanly.", C_GREEN, true));
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let os_args: Vec<String> = std::env::args().collect();
    if os_args.len() == 2 && (os_args[1] == "-V" || os_args[1] == "--version") {
        let version = colorful_version!();
        version.print_and_exit();
    }
    let args = Args::parse();
    term_ui::init(args.ascii);
    if args.version {
        println!("{}", ColoredVersion);
        return Ok(());
    }
    if args.list {
        return list_interfaces();
    }
    monitor_bandwidth(args)
}
