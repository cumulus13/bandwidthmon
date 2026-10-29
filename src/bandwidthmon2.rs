// File: src/bandwidthmon2.rs
//! Bandwidth Monitor 2 - Manual graph rendering
//! Author: Hadi Cahyadi <cumulus13@gmail.com>
//! License: MIT

use anyhow::{Context, Result};
#[path = "term_ui.rs"]
mod term_ui;

use clap::Parser;
use clap::ArgAction;
use clap_version_flag::colorful_version;
use clap_color_help::default_styles;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    style::Color,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::Networks;
use term_ui::Screen;
use std::fmt;

const INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_HISTORY: usize = 120;
const DEFAULT_HEIGHT: usize = 10;

struct ColoredVersion;

impl ColoredVersion {
    pub fn new() -> Self {
        Self {}
    }
}

impl fmt::Display for ColoredVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = style_text("bandwidthmon2", Color::Yellow, true);
        let author = style_text("Hadi Cahyadi <cumulus13@gmail.com>", Color::Cyan, true);
        let version = style_text(env!("CARGO_PKG_VERSION"), Color::White, true);

        write!(f, "{} {} by {}", name, version, author)
    }
}

#[derive(Parser, Debug)]
#[command(
    // name = "bandwidthmon2 by Hadi Cahyadi <cumulus13@gmail.com>",
    // version,
    // author = "Hadi Cahyadi <cumulus13@gmail.com>",
    about = "Real-time network bandwidth monitor with manual graph rendering",
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

    /// Plain output: one text line per second (auto when there is no TTY)
    #[arg(long)]
    plain: bool,

    /// Force ASCII-only drawing (auto when the locale/TERM is not UTF-8)
    #[arg(long)]
    ascii: bool,

    #[arg(short = 'v', short = 'V', long = "version", action = ArgAction::SetTrue)]
    version: bool,
}

#[derive(Debug, Clone)]
struct BandwidthStats {
    download_bps: f64,
    upload_bps: f64,
    total_rx: u64,
    total_tx: u64,
}

struct NetworkMonitor {
    interface: String,
    networks: Networks,
    history_dl: VecDeque<f64>,
    history_ul: VecDeque<f64>,
    prev_rx: u64,
    prev_tx: u64,
    prev_time: Instant,  // FIX: Track waktu untuk perhitungan akurat
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
        
        if !networks.iter().any(|(name, _)| name == &interface) {
            anyhow::bail!("Interface '{}' not found", interface);
        }

        let (prev_rx, prev_tx) = networks
            .get(&interface)
            .map(|data| (data.total_received(), data.total_transmitted()))
            .unwrap_or((0, 0));

        let now = Instant::now();

        Ok(Self {
            interface,
            networks,
            history_dl: VecDeque::with_capacity(history_size),
            history_ul: VecDeque::with_capacity(history_size),
            prev_rx,
            prev_tx,
            prev_time: now,  // FIX: Inisialisasi prev_time
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

        // FIX: Hitung waktu elapsed yang sebenarnya
        let elapsed = cur_time.duration_since(self.prev_time).as_secs_f64();
        
        // FIX: Hindari division by zero
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

        // FIX: Konversi ke bytes per second yang AKURAT
        let dl_bps = (dl_bytes as f64) / elapsed;
        let ul_bps = (ul_bytes as f64) / elapsed;

        self.prev_rx = cur_rx;
        self.prev_tx = cur_tx;
        self.prev_time = cur_time;  // FIX: Update prev_time

        // Update history
        if self.history_dl.len() >= self.history_dl.capacity() {
            self.history_dl.pop_front();
        }
        self.history_dl.push_back(dl_bps);

        if self.history_ul.len() >= self.history_ul.capacity() {
            self.history_ul.pop_front();
        }
        self.history_ul.push_back(ul_bps);

        // Update statistics
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

    fn get_history_dl(&self) -> Vec<f64> {
        self.history_dl.iter().copied().collect()
    }

    fn get_history_ul(&self) -> Vec<f64> {
        self.history_ul.iter().copied().collect()
    }
}

fn list_interfaces() -> Result<()> {
    let networks = Networks::new_with_refreshed_list();
    
    println!("\n{}", style_text("Available Network Interfaces:", Color::Cyan, true));
    println!("{}", "─".repeat(80));

    for (name, data) in networks.iter() {
        println!(
            "  {} {}",
            style_text(name, Color::White, true),
            style_text(
                &format!("(RX: {} bytes, TX: {} bytes)", 
                    data.total_received(), 
                    data.total_transmitted()
                ),
                Color::DarkGrey,
                false
            )
        );
    }
    println!();

    Ok(())
}

fn select_best_interface() -> Result<String> {
    let networks = Networks::new_with_refreshed_list();
    
    networks
        .iter()
        .max_by_key(|(_, data)| data.total_received() + data.total_transmitted())
        .map(|(name, _)| name.clone())
        .context("No network interfaces found")
}

fn resolve_interface(pattern: &str) -> Result<String> {
    let networks = Networks::new_with_refreshed_list();
    let interfaces: Vec<String> = networks.iter().map(|(name, _)| name.clone()).collect();
    
    // 1. Exact match
    if interfaces.iter().any(|name| name == pattern) {
        return Ok(pattern.to_string());
    }
    
    // 2. Case-insensitive partial match
    let pattern_lower = pattern.to_lowercase();
    let matches: Vec<String> = interfaces
        .iter()
        .filter(|name| name.to_lowercase().contains(&pattern_lower))
        .cloned()
        .collect();
    
    if matches.is_empty() {
        anyhow::bail!(
            "No interface matches '{}'. Available interfaces:\n{}",
            pattern,
            interfaces.join("\n")
        );
    }
    
    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }
    
    // Multiple matches - return the shortest one (most specific)
    Ok(matches
        .into_iter()
        .min_by_key(|s| s.len())
        .unwrap())
}

fn format_bytes(bytes: f64) -> String {
    const UNITS: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = bytes;
    let mut unit_idx = 0;

    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }

    format!("{:>7.2} {}", value, UNITS[unit_idx])
}

fn format_total_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0;

    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", value, UNITS[unit_idx])
}

fn style_text(text: &str, color: Color, bold: bool) -> String {
    term_ui::styled(text, color_to_256(color), bold)
}

fn color_to_256(color: Color) -> u8 {
    match color {
        Color::Cyan => 51,
        Color::Yellow => 226,
        Color::White => 15,
        Color::DarkGrey => 240,
        Color::Green => 46,
        Color::Magenta => 201,
        _ => 15,
    }
}

/// FIX: Improved graph rendering with proper alignment and smooth gradients
fn render_chart(data: &[f64], height: usize, width: usize, color: Color) -> String {
    if data.is_empty() || height == 0 || width == 0 {
        return String::new();
    }

    // Get the last `width` points
    let start_idx = data.len().saturating_sub(width);
    let plot_data: Vec<f64> = data[start_idx..].to_vec();

    if plot_data.is_empty() {
        return String::new();
    }

    // Calculate min and max
    let min_val = plot_data.iter().copied().fold(f64::INFINITY, f64::min);
    let max_val = plot_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    // Handle edge cases
    if !min_val.is_finite() || !max_val.is_finite() {
        return "Invalid data".to_string();
    }

    let range = if (max_val - min_val).abs() < f64::EPSILON {
        1.0
    } else {
        max_val - min_val
    };

    // FIX: Use better block characters for smooth gradient effect
    const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    
    // Initialize canvas with spaces
    let mut canvas: Vec<Vec<char>> = vec![vec![' '; width]; height];

    // Scale and plot data points with sub-character resolution
    // let scale = (height as f64) / range;

    for (x, &value) in plot_data.iter().enumerate() {
        if !value.is_finite() {
            continue;
        }

        // Calculate precise y position (inverted, 0 is top)
        let normalized = (value - min_val) / range;
        let y_float = (1.0 - normalized) * (height as f64);
        
        // Get integer and fractional parts for smooth rendering
        let y_int = y_float.floor() as usize;
        let y_frac = y_float - y_float.floor();
        
        // Main block
        if y_int < height {
            canvas[y_int][x] = '█';
        }
        
        // Fill below with full blocks
        for y in (y_int + 1)..height {
            canvas[y][x] = '█';
        }
        
        // Add gradient block at the top if there's fractional part
        if y_int > 0 && y_frac > 0.1 {
            let prev_y = y_int - 1;
            if canvas[prev_y][x] == ' ' {
                let block_idx = ((1.0 - y_frac) * 8.0) as usize;
                canvas[prev_y][x] = BLOCKS[block_idx.min(8)];
            }
        }
    }

    // FIX: Format labels with consistent width for perfect alignment
    let format_label = |val: f64| -> String {
        if val >= 1_000_000.0 {
            format!("{:>6.1}M", val / 1_000_000.0)
        } else if val >= 1_000.0 {
            format!("{:>6.1}K", val / 1_000.0)
        } else {
            format!("{:>7.1}", val)
        }
    };

    let label_max = format_label(max_val);
    let label_min = format_label(min_val);
    let label_mid = format_label((max_val + min_val) / 2.0);

    let color_code = color_to_256(color);
    let mut output = String::new();

    // FIX: Use ASCII pipe character for perfect vertical alignment
    for (row_idx, row) in canvas.iter().enumerate() {
        let label = if row_idx == 0 {
            &label_max
        } else if row_idx == height - 1 {
            &label_min
        } else if row_idx == height / 2 {
            &label_mid
        } else {
            "       " // 7 spaces to match label width
        };

        let line: String = row.iter().collect();
        // FIX: Use simple ASCII '|' for vertical line - always aligned
        output.push_str(&format!(
            "{} {}\n",
            label,
            term_ui::styled(&format!("|{}", line), color_code, false)
        ));
    }

    output
}

/// Whole UI as one string ('\n'-separated). Screen::draw() clips/draws it.
fn render_ui(monitor: &NetworkMonitor, stats: &BandwidthStats, args: &Args) -> String {
    let (term_width, rows) = term_ui::term_size();
    let both = !args.download && !args.upload;
    let show_dl = args.download || both;
    let show_ul = args.upload || both;
    let charts = show_dl as usize + show_ul as usize;
    let (height, show_summary) =
        term_ui::fit_chart_height(rows, args.height, charts, args.summary, 0);
    let chart_width = if args.width > 0 {
        args.width
    } else {
        term_width.saturating_sub(15).max(10)
    };

    let mut l: Vec<String> = Vec::new();

    l.push(style_text(
        &format!("═══ Bandwidth Monitor ({}) ═══", monitor.interface),
        Color::Cyan,
        true,
    ));

    let hint = if term_width >= 78 {
        "'q'/Ctrl+C=quit"
    } else if term_width >= 56 {
        "q=quit"
    } else {
        ""
    };
    l.push(format!(
        "{} {}  │  {} {}  {}",
        style_text("Download:", Color::Cyan, true),
        style_text(&format_bytes(stats.download_bps), Color::White, false),
        style_text("Upload:", Color::Yellow, true),
        style_text(&format_bytes(stats.upload_bps), Color::White, false),
        style_text(hint, Color::DarkGrey, false)
    ));

    if show_summary {
        l.push(format!(
            "{} {}  │  {} {}",
            style_text("Peak DL:", Color::Cyan, false),
            style_text(&format_bytes(monitor.peak_dl), Color::White, false),
            style_text("Peak UL:", Color::Yellow, false),
            style_text(&format_bytes(monitor.peak_ul), Color::White, false),
        ));
        l.push(format!(
            "{} {}  │  {} {}",
            style_text("Avg DL:", Color::Cyan, false),
            style_text(&format_bytes(monitor.avg_dl), Color::White, false),
            style_text("Avg UL:", Color::Yellow, false),
            style_text(&format_bytes(monitor.avg_ul), Color::White, false),
        ));
        l.push(format!(
            "{} {}  │  {} {}",
            style_text("Total RX:", Color::Cyan, false),
            style_text(&format_total_bytes(stats.total_rx), Color::White, false),
            style_text("Total TX:", Color::Yellow, false),
            style_text(&format_total_bytes(stats.total_tx), Color::White, false),
        ));
        l.push(format!(
            "{} {:.1}s",
            style_text("Runtime:", Color::Green, false),
            monitor.start_time.elapsed().as_secs_f64()
        ));
    }

    l.push(String::new());

    if show_dl {
        let dl = monitor.get_history_dl();
        if !dl.is_empty() {
            l.push(render_chart(&dl, height, chart_width, Color::Cyan).trim_end_matches('\n').to_string());
        }
        if show_ul {
            l.push(String::new());
        }
    }
    if show_ul {
        let ul = monitor.get_history_ul();
        if !ul.is_empty() {
            l.push(render_chart(&ul, height, chart_width, Color::Yellow).trim_end_matches('\n').to_string());
        }
    }

    l.join("\n")
}

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

        if last_update.elapsed() >= INTERVAL {
            let stats = monitor.update()?;
            last_update = Instant::now();

            if screen.is_some() {
                last_stats = Some(stats);
                redraw = true;
            } else {
                let line = term_ui::plain_line(
                    &monitor.interface,
                    monitor.start_time.elapsed().as_secs(),
                    &format_bytes(stats.download_bps),
                    &format_bytes(stats.upload_bps),
                );
                if !term_ui::emit_plain(&line) {
                    break; // stdout closed (e.g. `| head`)
                }
            }
        }

        if redraw {
            if let Some(s) = screen.as_mut() {
                let frame = match &last_stats {
                    Some(st) => render_ui(monitor, st, args),
                    None => format!(
                        "{}\n{}",
                        style_text(&format!("═══ Bandwidth Monitor ({}) ═══", monitor.interface), Color::Cyan, true),
                        style_text("collecting first sample…  (q to quit)", Color::DarkGrey, false)
                    ),
                };
                s.draw(&frame)?;
            }
            redraw = false;
        }
    }
    Ok(())
}

fn monitor_bandwidth(args: Args) -> Result<()> {
    let interface = if let Some(iface) = args.iface.clone() {
        resolve_interface(&iface)?
    } else {
        select_best_interface()?
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    // "termination" feature: SIGTERM/SIGHUP (ssh drop) also stop cleanly.
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst))?;

    let mut monitor = NetworkMonitor::new(interface, args.history)?;

    let mut screen = Screen::try_enter(args.plain);
    if screen.is_none() {
        eprintln!(
            "{} {}  (plain mode, Ctrl-C to stop)",
            style_text("Monitoring interface:", Color::Green, false),
            style_text(&monitor.interface, Color::Cyan, true)
        );
    }

    let result = run_loop(&mut monitor, &args, &running, &mut screen);

    // Restore the terminal BEFORE printing anything else.
    drop(screen);

    if let Err(e) = result {
        eprintln!("{} {}", style_text("Error:", Color::Red, true), e);
        std::process::exit(1);
    }
    eprintln!("{}", style_text("Stopped cleanly.", Color::Green, true));
    Ok(())
}

fn main() -> Result<()> {
    let os_args: Vec<String> = std::env::args().collect();
    if os_args.len() == 2 && (os_args[1] == "-V" || os_args[1] == "--version") {
        let version = colorful_version!();
        version.print_and_exit();
    }
    let args = Args::parse();
    term_ui::init(args.ascii);

    if args.version {
        println!("{}", ColoredVersion::new());
        return Ok(());
    }

    if args.list {
        list_interfaces()?;
        return Ok(());
    }

    monitor_bandwidth(args)
}
