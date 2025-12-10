#!/usr/bin/env rust
//! Bandwidth Monitor - Using rasciichart library
//! Author: Hadi Cahyadi <cumulus13@gmail.com>
//! License: MIT

use crossterm::queue;
use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute,
    style::{Color, Print},
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use rasciichart::{plot_with_config, Config as ChartConfig};
use std::collections::VecDeque;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::Networks;

// const INTERVAL: Duration = Duration::from_secs(1);
const INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_HISTORY: usize = 120;
const DEFAULT_HEIGHT: usize = 10;

#[derive(Parser, Debug)]
#[command(
    name = "bandwidthmon",
    version,
    author = "Hadi Cahyadi <cumulus13@gmail.com>",
    about = "Real-time network bandwidth monitor with ASCII charts by Hadi Cahyadi <cumulus13@gmail.com>"
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
        self.networks.refresh();

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

        // FIX: Update history dengan mekanisme yang benar
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
    if bold {
        format!("\x1b[1m\x1b[38;5;{}m{}\x1b[0m", color_to_256(color), text)
    } else {
        format!("\x1b[38;5;{}m{}\x1b[0m", color_to_256(color), text)
    }
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

fn render_ui(
    monitor: &NetworkMonitor,
    stats: &BandwidthStats,
    args: &Args,
    term_width: u16,
) -> Result<String> {
    let mut output = String::new();
    let min_chart_width = 10;
    let chart_width = if args.width > 0 {
        args.width.clamp(min_chart_width, term_width.saturating_sub(10).max(20) as usize)
    } else {
        term_width.saturating_sub(15).max(20) as usize
    };

    // Header
    output.push_str(&format!(
        "{}\n",
        style_text(
            &format!("═══ Bandwidth Monitor ({}) ═══", monitor.interface),
            Color::Cyan,
            true
        )
    ));

    // Current speeds
    output.push_str(&format!(
        "{} {}  │  {} {}  {}\n",
        style_text("Download:", Color::Cyan, true),
        style_text(&format_bytes(stats.download_bps), Color::White, false),
        style_text("Upload:", Color::Yellow, true),
        style_text(&format_bytes(stats.upload_bps), Color::White, false),
        style_text("Press 'q' or Ctrl+C to quit", Color::DarkGrey, false)
    ));

    if args.summary {
        output.push_str(&format!(
            "{} {}  │  {} {}\n",
            style_text("Peak DL:", Color::Cyan, false),
            style_text(&format_bytes(monitor.peak_dl), Color::White, false),
            style_text("Peak UL:", Color::Yellow, false),
            style_text(&format_bytes(monitor.peak_ul), Color::White, false),
        ));
        output.push_str(&format!(
            "{} {}  │  {} {}\n",
            style_text("Avg DL:", Color::Cyan, false),
            style_text(&format_bytes(monitor.avg_dl), Color::White, false),
            style_text("Avg UL:", Color::Yellow, false),
            style_text(&format_bytes(monitor.avg_ul), Color::White, false),
        ));
        output.push_str(&format!(
            "{} {}  │  {} {}\n",
            style_text("Total RX:", Color::Cyan, false),
            style_text(&format_total_bytes(stats.total_rx), Color::White, false),
            style_text("Total TX:", Color::Yellow, false),
            style_text(&format_total_bytes(stats.total_tx), Color::White, false),
        ));
        output.push_str(&format!(
            "{} {:.1}s\n",
            style_text("Runtime:", Color::Green, false),
            monitor.start_time.elapsed().as_secs_f64()
        ));
    }

    output.push('\n');

    // Charts
    let config = ChartConfig::default()
        .with_height(args.height)
        .with_width(chart_width)
        .with_labels(true);

    let show_both = !args.download && !args.upload;

    if args.download || show_both {
        let dl_history = monitor.get_history_dl();

        if !dl_history.is_empty() {
            let color_code = color_to_256(Color::Cyan);

            match plot_with_config(&dl_history, config.clone()) {
                Ok(chart) => {
                    let colored = format!("\x1b[38;5;{}m{}\x1b[0m", color_code, chart);
                    output.push_str(&colored);
                    if show_both {output.push('\n');}
                }
                Err(e) => {
                    output.push_str(&format!("Chart error: {}\n", e));
                }
            }
        }
    }

    if (args.upload || show_both) && !args.download {
        if show_both {
            output.push('\n');
        }
        let ul_history = monitor.get_history_ul();
        if !ul_history.is_empty() {
            let color_code = color_to_256(Color::Yellow);

            match plot_with_config(&ul_history, config.clone()) {
                Ok(chart) => {
                    let colored = format!("\x1b[38;5;{}m{}\x1b[0m", color_code, chart);
                    output.push_str(&colored);
                }
                Err(e) => {
                    output.push_str(&format!("Chart error: {}\n", e));
                }
            }
        }
    }

    Ok(output)
}

fn monitor_bandwidth(args: Args) -> Result<()> {
    let interface = if let Some(iface) = args.iface.clone() {
        resolve_interface(&iface)?
    } else {
        select_best_interface()?
    };

    println!("Monitoring interface: {}\n", style_text(&interface, Color::Cyan, true));

    let mut monitor = NetworkMonitor::new(interface, args.history)?;
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    enable_raw_mode()?;

    let result = (|| -> Result<()> {
        let mut last_update = Instant::now();

        while running.load(Ordering::SeqCst) {
            // Check for key events (non-blocking)
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                        KeyCode::Char('c') => {
                            use crossterm::event::KeyModifiers;
                            if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }

            // FIX: Update bandwidth stats dengan timing yang akurat
            if last_update.elapsed() >= INTERVAL {
                let stats = monitor.update()?;
                let (term_width, term_height) = size()?;

                let ui = render_ui(&monitor, &stats, &args, term_width)?;
                let mut lines: Vec<String> = ui.lines().map(str::to_owned).collect();

                // Pastikan tepat term_height baris
                lines.resize_with(term_height as usize, String::new);

                let full_output = lines.join("\n");

                queue!(
                    stdout,
                    MoveTo(0, 0),
                    Print(full_output)
                )?;
                stdout.flush()?;

                last_update = Instant::now();
            }
        }
        Ok(())
    })();

    // Cleanup
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, Show)?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    } else {
        println!("\n{}", style_text("Stopped cleanly.", Color::Green, true));
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.list {
        list_interfaces()?;
        return Ok(());
    }

    monitor_bandwidth(args)
}