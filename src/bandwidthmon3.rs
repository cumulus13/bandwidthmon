// File: src/bandwidthmon3.rs
// Real-time Bandwidth Monitor with ASCII Chart
// Author: Hadi Cahyadi <cumulus13@gmail.com>

use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use std::thread;
use std::io::{self, Write, stdin};
use clap::Parser;
use colored::*;
use rasciichart::{plot_with_config, Config};
use regex::Regex;
use sysinfo::Networks;

#[derive(Parser, Debug)]
#[clap(author, version, about = "Bandwidth monitor with real-time chart")]
struct Args {
    /// Network interface (supports pattern/regex/wildcard)
    #[clap(short, long, default_value = "")]
    interface: String,
    
    /// Chart height
    #[clap(short = 'H', long, default_value = "15")]
    height: usize,
    
    /// Chart width (0 = auto, adjusts to terminal)
    #[clap(short = 'W', long, default_value = "0")]
    width: usize,
    
    /// Interval between measurements (seconds)
    #[clap(short = 't', long, default_value = "1.0")]
    interval: f64,
    
    /// Show download only
    #[clap(short = 'd', long)]
    download_only: bool,
    
    /// Show upload only
    #[clap(short = 'u', long)]
    upload_only: bool,
    
    /// Show summary statistics
    #[clap(short = 's', long)]
    show_summary: bool,
    
    /// Static mode: simple line-by-line output without chart
    #[clap(long)]
    static_mode: bool,
    
    /// Chart-only mode: only show chart and current status
    #[clap(short, long)]
    chart_only: bool,
    
    /// Show version and author
    #[clap(short = 'v', long)]
    version: bool,
    
    /// List available network interfaces
    #[clap(short = 'l', long)]
    list: bool,
}

#[derive(Clone)]
struct Stats {
    samples: u64,
    total_download: f64,
    total_upload: f64,
    min_download: f64,
    max_download: f64,
    min_upload: f64,
    max_upload: f64,
    download_rates: Vec<f64>,
    upload_rates: Vec<f64>,
}

impl Stats {
    fn new() -> Self {
        Self {
            samples: 0,
            total_download: 0.0,
            total_upload: 0.0,
            min_download: f64::INFINITY,
            max_download: 0.0,
            min_upload: f64::INFINITY,
            max_upload: 0.0,
            download_rates: Vec::new(),
            upload_rates: Vec::new(),
        }
    }

    fn avg_download(&self) -> f64 {
        if self.download_rates.is_empty() { 
            0.0 
        } else { 
            self.download_rates.iter().sum::<f64>() / self.download_rates.len() as f64 
        }
    }

    fn avg_upload(&self) -> f64 {
        if self.upload_rates.is_empty() { 
            0.0 
        } else { 
            self.upload_rates.iter().sum::<f64>() / self.upload_rates.len() as f64 
        }
    }

    fn stddev_download(&self) -> f64 {
        if self.download_rates.len() < 2 { 
            return 0.0; 
        }
        let avg = self.avg_download();
        let variance = self.download_rates.iter()
            .map(|&x| (x - avg).powi(2))
            .sum::<f64>() / (self.download_rates.len() - 1) as f64;
        variance.sqrt()
    }

    fn stddev_upload(&self) -> f64 {
        if self.upload_rates.len() < 2 { 
            return 0.0; 
        }
        let avg = self.avg_upload();
        let variance = self.upload_rates.iter()
            .map(|&x| (x - avg).powi(2))
            .sum::<f64>() / (self.upload_rates.len() - 1) as f64;
        variance.sqrt()
    }
}

#[derive(Clone)]
struct NetStats {
    rx_bytes: u64,
    tx_bytes: u64,
}

fn read_version_file() -> Option<String> {
    let version_path = Path::new("VERSION");
    if let Ok(content) = fs::read_to_string(version_path) {
        // Parse "version = x.y.z"
        for line in content.lines() {
            if let Some(stripped) = line.trim().strip_prefix("version") {
                if let Some(version) = stripped.trim().strip_prefix('=') {
                    return Some(version.trim().to_string());
                }
            }
        }
    }
    None
}

fn print_version_info() {
    let version = read_version_file().unwrap_or_else(|| "unknown".to_string());
    println!("{}", "Bandwidth Monitor".bright_magenta().bold());
    println!("Version: {}", version.bright_cyan());
    println!("Author: {}", "Hadi Cahyadi <cumulus13@gmail.com>".bright_yellow());
}

fn print_interfaces_list() {
    match list_interfaces() {
        Ok(interfaces) => {
            println!("{}", "Available Network Interfaces:".bright_cyan().bold());
            println!();
            for (idx, iface) in interfaces.iter().enumerate() {
                println!("  {}. {}", 
                    format!("{}", idx + 1).bright_yellow(),
                    iface.bright_green()
                );
            }
            println!();
            println!("{}", "Tip: Use -i with interface name or pattern".bright_black());
            println!("{}", "     - Partial match: -i 'realtek' matches 'vEthernet (realtek)'".bright_black());
            println!("{}", "     - Wildcards: -i 'Wi*' or -i 'vEthernet*'".bright_black());
        }
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            std::process::exit(1);
        }
    }
}

#[cfg(target_os = "linux")]
fn list_interfaces() -> Result<Vec<String>, String> {
    let sys_net = Path::new("/sys/class/net");
    if !sys_net.exists() {
        return Err("Cannot access /sys/class/net".to_string());
    }

    let entries = fs::read_dir(sys_net)
        .map_err(|e| format!("Failed to read network interfaces: {}", e))?;

    let mut interfaces = Vec::new();
    for entry in entries {
        if let Ok(entry) = entry {
            if let Ok(name) = entry.file_name().into_string() {
                interfaces.push(name);
            }
        }
    }
    
    Ok(interfaces)
}

#[cfg(target_os = "windows")]
fn list_interfaces() -> Result<Vec<String>, String> {
    let mut networks = Networks::new_with_refreshed_list();
    networks.refresh(false);
    
    let mut interfaces: Vec<String> = networks.iter()
        .map(|(name, _)| name.to_string())
        .collect();
    
    interfaces.sort();
    Ok(interfaces)
}

fn find_matching_interface(pattern: &str) -> Result<String, String> {
    let interfaces = list_interfaces()?;
    
    if interfaces.is_empty() {
        return Err("No network interfaces found".to_string());
    }

    // If no pattern specified, use first interface
    if pattern.is_empty() {
        return Ok(interfaces[0].clone());
    }

    // Check if pattern contains wildcards
    let has_wildcards = pattern.contains('*') || pattern.contains('?');
    
    if has_wildcards {
        // Use regex for wildcard patterns - exact match
        let regex_pattern = pattern
            .replace(".", "\\.")
            .replace("(", "\\(")
            .replace(")", "\\)")
            .replace("*", ".*")
            .replace("?", ".");
        
        let re = Regex::new(&format!("(?i)^{}$", regex_pattern))
            .map_err(|e| format!("Invalid pattern: {}", e))?;

        for iface in &interfaces {
            if re.is_match(iface) {
                return Ok(iface.clone());
            }
        }
    } else {
        // No wildcards - try partial match (case insensitive)
        let pattern_lower = pattern.to_lowercase();
        
        for iface in &interfaces {
            if iface.to_lowercase().contains(&pattern_lower) {
                return Ok(iface.clone());
            }
        }
    }

    Err(format!("No interface matching '{}' found. Available: {}", pattern, interfaces.join(", ")))
}

#[cfg(target_os = "linux")]
fn read_interface_stats(interface: &str) -> Result<NetStats, String> {
    let rx_path = format!("/sys/class/net/{}/statistics/rx_bytes", interface);
    let tx_path = format!("/sys/class/net/{}/statistics/tx_bytes", interface);

    let rx_bytes = fs::read_to_string(&rx_path)
        .map_err(|e| format!("Failed to read rx_bytes: {}", e))?
        .trim()
        .parse::<u64>()
        .map_err(|e| format!("Failed to parse rx_bytes: {}", e))?;

    let tx_bytes = fs::read_to_string(&tx_path)
        .map_err(|e| format!("Failed to read tx_bytes: {}", e))?
        .trim()
        .parse::<u64>()
        .map_err(|e| format!("Failed to parse tx_bytes: {}", e))?;

    Ok(NetStats { rx_bytes, tx_bytes })
}

#[cfg(target_os = "windows")]
fn read_interface_stats(interface: &str) -> Result<NetStats, String> {
    let mut networks = Networks::new_with_refreshed_list();
    networks.refresh(false);
    
    if let Some(network) = networks.get(interface) {
        Ok(NetStats {
            rx_bytes: network.total_received(),
            tx_bytes: network.total_transmitted(),
        })
    } else {
        Err(format!("Interface '{}' not found", interface))
    }
}

fn bytes_to_human(bytes: f64) -> String {
    const UNITS: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s"];
    let mut size = bytes;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

fn get_term_size() -> (u16, u16) {
    term_size::dimensions()
        .map(|(w, h)| (w as u16, h as u16))
        .unwrap_or((80, 24))
}

fn move_cursor_home() {
    print!("\x1B[H");
    let _ = io::stdout().flush();
}

fn clear_line_to_end() {
    print!("\x1B[K");
}

fn render_static_line(stats: &Stats, download_rate: f64, upload_rate: f64, args: &Args) {
    print!("sample={} ", format!("{}", stats.samples).cyan());
    
    if !args.upload_only {
        print!("↓ {} ", bytes_to_human(download_rate).truecolor(0, 255, 255));
    }
    
    if !args.download_only {
        print!("↑ {} ", bytes_to_human(upload_rate).truecolor(255, 255, 0));
    }
    
    if args.show_summary && stats.samples > 0 {
        print!("(avg: ");
        if !args.upload_only {
            print!("↓{} ", bytes_to_human(stats.avg_download()).truecolor(0, 255, 255));
        }
        if !args.download_only {
            print!("↑{}", bytes_to_human(stats.avg_upload()).truecolor(255, 255, 0));
        }
        print!(")");
    }
    
    println!();
}

fn render_chart_only(args: &Args, download_history: &VecDeque<f64>, upload_history: &VecDeque<f64>, 
                     download_rate: f64, upload_rate: f64, interface: &str, chart_width: usize) {
    move_cursor_home();

    // Status line with quit hint
    print!("{} ", "Interface:".bright_magenta().bold());
    print!("{}", format!(" {} ", interface).black().on_bright_magenta());
    print!(" | ");
    
    if !args.upload_only {
        print!("{} ", "Download:".bold());
        print!("{}", format!(" {} ", bytes_to_human(download_rate)).black().on_truecolor(0, 255, 255));
        if args.download_only {
            print!("  ");
        } else {
            print!(" │ ");
        }
    }
    
    if !args.download_only {
        print!("{} ", "Upload:".bold());
        print!("{}", format!(" {} ", bytes_to_human(upload_rate)).black().on_truecolor(255, 255, 0));
        print!("  ");
    }
    
    print!("{}", "Press 'q' or Ctrl+C to quit".bright_black());
    clear_line_to_end();
    println!();

    // Charts
    if !args.upload_only && download_history.len() > 1 {
        print!("{}", "Download History:".truecolor(0, 255, 255).bold());
        clear_line_to_end();
        println!();
        
        let data: Vec<f64> = download_history.iter().copied().collect();
        let config = Config::new()
            .with_height(args.height)
            .with_width(chart_width);
        
        if let Ok(chart) = plot_with_config(&data, config) {
            let lines: Vec<&str> = chart.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                print!("{}", line.truecolor(0, 255, 255));
                clear_line_to_end();
                if i < lines.len() - 1 {
                    println!();
                }
            }
        }
        if !args.download_only {
            println!();
        }
    }

    if !args.download_only && upload_history.len() > 1 {
        print!("{}", "Upload History:".truecolor(255, 255, 0).bold());
        clear_line_to_end();
        println!();
        
        let data: Vec<f64> = upload_history.iter().copied().collect();
        let config = Config::new()
            .with_height(args.height)
            .with_width(chart_width);
        
        if let Ok(chart) = plot_with_config(&data, config) {
            let lines: Vec<&str> = chart.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                print!("{}", line.truecolor(255, 255, 0));
                clear_line_to_end();
                if i < lines.len() - 1 {
                    println!();
                }
            }
        }
    }

    print!("\x1B[J");
    let _ = io::stdout().flush();
}

fn render_dynamic_screen(args: &Args, stats: &Stats, download_history: &VecDeque<f64>, 
                        upload_history: &VecDeque<f64>, download_rate: f64, upload_rate: f64, 
                        interface: &str, chart_width: usize) {
    move_cursor_home();

    // Header with quit hint
    print!("{}", format!("=== Real-time Bandwidth Monitor: {} ===", interface).bright_magenta().bold());
    clear_line_to_end();
    println!();

    // Status line
    if !args.upload_only {
        print!("{} ", "Download:".truecolor(0, 255, 255).bold());
        print!("{}", format!(" {} ", bytes_to_human(download_rate)).black().on_truecolor(0, 255, 255));
        if args.download_only {
            print!("  ");
        } else {
            print!(" │ ");
        }
    }
    
    if !args.download_only {
        print!("{} ", "Upload:".truecolor(255, 255, 0).bold());
        print!("{}", format!(" {} ", bytes_to_human(upload_rate)).black().on_truecolor(255, 255, 0));
        print!("  ");
    }
    
    print!("{}", "Press 'q' or Ctrl+C to quit".bright_black());
    clear_line_to_end();
    println!();

    // Statistics
    if args.show_summary {
        print!("{}", "Statistics:".bright_yellow().bold());
        clear_line_to_end();
        println!();
        print!("  Samples: {}", format!("{}", stats.samples).cyan());
        clear_line_to_end();
        println!();

        if !args.upload_only && !stats.download_rates.is_empty() {
            print!("  Download: Min={} | Avg={} | Max={} | StdDev={}",
                bytes_to_human(stats.min_download).green(),
                bytes_to_human(stats.avg_download()).yellow(),
                bytes_to_human(stats.max_download).red(),
                bytes_to_human(stats.stddev_download()).cyan()
            );
            clear_line_to_end();
            println!();
        }

        if !args.download_only && !stats.upload_rates.is_empty() {
            print!("  Upload:   Min={} | Avg={} | Max={} | StdDev={}",
                bytes_to_human(stats.min_upload).green(),
                bytes_to_human(stats.avg_upload()).yellow(),
                bytes_to_human(stats.max_upload).red(),
                bytes_to_human(stats.stddev_upload()).cyan()
            );
            clear_line_to_end();
            println!();
        }
    }

    // Charts
    if !args.upload_only && download_history.len() > 1 {
        print!("{}", "Download History:".truecolor(0, 255, 255).bold());
        clear_line_to_end();
        println!();
        
        let data: Vec<f64> = download_history.iter().copied().collect();
        let config = Config::new()
            .with_height(args.height)
            .with_width(chart_width);
        
        if let Ok(chart) = plot_with_config(&data, config) {
            let lines: Vec<&str> = chart.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                print!("{}", line.truecolor(0, 255, 255));
                clear_line_to_end();
                if i < lines.len() - 1 {
                    println!();
                }
            }
        }
        if !args.download_only {
            println!();
        }
    }

    if !args.download_only && upload_history.len() > 1 {
        print!("{}", "Upload History:".truecolor(255, 255, 0).bold());
        clear_line_to_end();
        println!();
        
        let data: Vec<f64> = upload_history.iter().copied().collect();
        let config = Config::new()
            .with_height(args.height)
            .with_width(chart_width);
        
        if let Ok(chart) = plot_with_config(&data, config) {
            let lines: Vec<&str> = chart.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                print!("{}", line.truecolor(255, 255, 0));
                clear_line_to_end();
                if i < lines.len() - 1 {
                    println!();
                }
            }
        }
    }

    print!("\x1B[J");
    let _ = io::stdout().flush();
}

fn print_final_stats(stats: &Stats, args: &Args) {
    println!("\n{}", "✓ Stopped".green().bold());
    println!("\n{}", "Final Statistics:".bright_yellow().bold());
    println!("  Total Samples: {}", stats.samples);
    println!("  Total Data: Downloaded = {}, Uploaded = {}",
        bytes_to_human(stats.total_download),
        bytes_to_human(stats.total_upload)
    );

    if !args.upload_only && !stats.download_rates.is_empty() {
        println!("  Download Rate: Min = {}, Avg = {}, Max = {}, StdDev = {}",
            bytes_to_human(stats.min_download).green(),
            bytes_to_human(stats.avg_download()).yellow(),
            bytes_to_human(stats.max_download).red(),
            bytes_to_human(stats.stddev_download()).cyan()
        );
    }

    if !args.download_only && !stats.upload_rates.is_empty() {
        println!("  Upload Rate:   Min = {}, Avg = {}, Max = {}, StdDev = {}",
            bytes_to_human(stats.min_upload).green(),
            bytes_to_human(stats.avg_upload()).yellow(),
            bytes_to_human(stats.max_upload).red(),
            bytes_to_human(stats.stddev_upload()).cyan()
        );
    }
}

fn main() {
    let args = Args::parse();

    if args.version {
        print_version_info();
        return;
    }

    if args.list {
        print_interfaces_list();
        return;
    }

    // Find matching interface
    let interface = match find_matching_interface(&args.interface) {
        Ok(iface) => iface,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            eprintln!("\n{}", "Use -l or --list to see available interfaces".bright_yellow());
            std::process::exit(1);
        }
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    // Spawn thread to listen for 'q' key
    let r2 = running.clone();
    thread::spawn(move || {
        let stdin = stdin();
        loop {
            let mut buffer = String::new();
            if stdin.read_line(&mut buffer).is_ok() {
                if buffer.trim().eq_ignore_ascii_case("q") {
                    r2.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    });

    let mut stats = Stats::new();
    let mut download_history: VecDeque<f64> = VecDeque::new();
    let mut upload_history: VecDeque<f64> = VecDeque::new();

    // Initial setup
    if !args.static_mode && !args.chart_only {
        print!("\x1B[2J\x1B[H");
        let _ = io::stdout().flush();
    } else if args.static_mode {
        println!("{}", format!("Monitoring {} ...", interface).bright_magenta().bold());
    } else if args.chart_only {
        print!("\x1B[2J\x1B[H");
        let _ = io::stdout().flush();
    }

    // Get initial stats
    let mut prev_stats = match read_interface_stats(&interface) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            std::process::exit(1);
        }
    };

    let mut last_time = Instant::now();

    // Main loop
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs_f64(args.interval));

        // Get current terminal width for dynamic resize
        let (term_w, _) = get_term_size();
        let hist_size = if args.width > 0 { 
            args.width 
        } else { 
            (term_w as usize).saturating_sub(14).max(50) 
        };
        let chart_width = hist_size;

        let now = Instant::now();
        let elapsed = now.duration_since(last_time).as_secs_f64();
        last_time = now;

        let current_stats = match read_interface_stats(&interface) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let rx_diff = current_stats.rx_bytes.saturating_sub(prev_stats.rx_bytes) as f64;
        let tx_diff = current_stats.tx_bytes.saturating_sub(prev_stats.tx_bytes) as f64;

        let download_rate = rx_diff / elapsed;
        let upload_rate = tx_diff / elapsed;

        prev_stats = current_stats;
        stats.samples += 1;
        stats.total_download += rx_diff;
        stats.total_upload += tx_diff;

        if !args.upload_only {
            stats.min_download = stats.min_download.min(download_rate);
            stats.max_download = stats.max_download.max(download_rate);
            stats.download_rates.push(download_rate);
            download_history.push_back(download_rate);
            if download_history.len() > hist_size {
                download_history.pop_front();
            }
        }

        if !args.download_only {
            stats.min_upload = stats.min_upload.min(upload_rate);
            stats.max_upload = stats.max_upload.max(upload_rate);
            stats.upload_rates.push(upload_rate);
            upload_history.push_back(upload_rate);
            if upload_history.len() > hist_size {
                upload_history.pop_front();
            }
        }

        // Render output based on mode
        if args.static_mode {
            render_static_line(&stats, download_rate, upload_rate, &args);
        } else if args.chart_only {
            render_chart_only(&args, &download_history, &upload_history, download_rate, upload_rate, &interface, chart_width);
        } else {
            render_dynamic_screen(&args, &stats, &download_history, &upload_history, download_rate, upload_rate, &interface, chart_width);
        }

        if !running.load(Ordering::SeqCst) { 
            break; 
        }
    }

    // Print final statistics
    print_final_stats(&stats, &args);
}