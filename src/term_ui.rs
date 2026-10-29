//! Shared terminal layer for every bandwidthmon binary.
//! Author: Hadi Cahyadi <cumulus13@gmail.com>
//! License: MIT
//!
//! Why this exists — what goes wrong over SSH and how it is handled here:
//!
//! 1. Frame taller than the terminal (24-row ssh/putty/tmux pane): the frame
//!    scrolls, `cursor home` no longer lines up, headers pile up.
//!    -> `fit_chart_height()` sizes the charts to the real row count and
//!       `Screen::draw()` hard-clips the frame to `rows` lines.
//! 2. Line wider than the terminal wraps and shifts everything below it.
//!    -> every line is clipped to `cols - 1` visible chars (ANSI-aware) AND
//!       auto-wrap (DECAWM) is switched off while the UI is up.
//! 3. One write() per line: over a slow link the frame arrives in pieces and
//!    tears/flickers.
//!    -> the whole frame is built in memory and sent with ONE write + flush.
//! 4. Resize while running leaves stale glyphs.
//!    -> size change => full clear on the next frame.
//! 5. `ssh host cmd` without `-t` (no PTY): raw mode fails with ENXIO.
//!    -> `Screen::try_enter()` returns None and callers fall back to plain
//!       one-line-per-second output.
//! 6. Remote locale/TERM can't do UTF-8 box drawing / 256 colours.
//!    -> `Caps` detection + ASCII folding + 16-colour / no-colour fallback.
//! 7. Terminal left in raw mode / alt screen after Ctrl-C, SIGHUP, panic.
//!    -> RAII guard + panic hook always restore the terminal.
#![allow(dead_code)]

use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::{
    cursor::{Hide, Show},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

// ── Capabilities ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorLevel {
    None,
    Basic,
    Xterm256,
}

#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub utf8: bool,
    pub color: ColorLevel,
}

static CAPS: OnceLock<Caps> = OnceLock::new();

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn detect(force_ascii: bool) -> Caps {
    let term = env_nonempty("TERM").unwrap_or_default().to_lowercase();

    let utf8 = if force_ascii {
        false
    } else if cfg!(windows) {
        true
    } else {
        // POSIX precedence: LC_ALL > LC_CTYPE > LANG. Unset => assume UTF-8
        // (the terminal, not the remote locale, decides how bytes are shown).
        let locale_ok = match env_nonempty("LC_ALL")
            .or_else(|| env_nonempty("LC_CTYPE"))
            .or_else(|| env_nonempty("LANG"))
        {
            Some(l) => {
                let l = l.to_lowercase();
                l.contains("utf-8") || l.contains("utf8")
            }
            None => true,
        };
        let term_ok = !matches!(term.as_str(), "dumb" | "vt100" | "vt102" | "vt220");
        locale_ok && term_ok
    };

    let color = if env_nonempty("NO_COLOR").is_some() || term == "dumb" || !io::stdout().is_terminal() {
        ColorLevel::None
    } else if cfg!(windows) || term.contains("256") || env_nonempty("COLORTERM").is_some() {
        ColorLevel::Xterm256
    } else {
        ColorLevel::Basic
    };

    Caps { utf8, color }
}

/// Call once at startup (after parsing args). Later calls are ignored.
pub fn init(force_ascii: bool) {
    let _ = CAPS.set(detect(force_ascii));
}

pub fn caps() -> Caps {
    *CAPS.get_or_init(|| detect(false))
}

// ── Colour ────────────────────────────────────────────────────────────────────

fn basic_sgr(col256: u8) -> u8 {
    match col256 {
        51 => 96,  // cyan
        226 => 93, // yellow
        15 => 97,  // white
        240 => 90, // grey
        46 => 92,  // green
        201 => 95, // magenta
        196 => 91, // red
        _ => 39,
    }
}

/// Colour `text` with a 256-colour palette index, degraded to what the
/// terminal supports (16 colours, or none).
pub fn styled(text: &str, col256: u8, bold: bool) -> String {
    let b = if bold { "\x1b[1m" } else { "" };
    match caps().color {
        ColorLevel::None => text.to_string(),
        ColorLevel::Basic => format!("{}\x1b[{}m{}\x1b[0m", b, basic_sgr(col256), text),
        ColorLevel::Xterm256 => format!("{}\x1b[38;5;{}m{}\x1b[0m", b, col256, text),
    }
}

// ── Text helpers ──────────────────────────────────────────────────────────────

/// Replace box-drawing / block glyphs with ASCII (only used when !utf8).
pub fn fold_ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            c if c.is_ascii() => c,
            '═' => '=',
            '─' | '━' => '-',
            '│' | '║' | '┤' | '├' => '|',
            '╭' | '╮' | '╰' | '╯' | '┼' | '┬' | '┴' | '┌' | '┐' | '└' | '┘' => '+',
            '▼' => 'v',
            '▲' => '^',
            '▁' | '▂' | '▃' => '_',
            '▄' | '▅' | '▆' | '▇' | '█' => '#',
            _ => '?',
        })
        .collect()
}

/// Clip `s` to `max` visible columns, skipping CSI escape sequences when
/// counting. Appends a colour reset if anything was cut.
pub fn truncate_visible(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut width = 0usize;
    let mut cut = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            out.push(c);
            if it.peek() == Some(&'[') {
                out.push('[');
                it.next();
                while let Some(&n) = it.peek() {
                    out.push(n);
                    it.next();
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
            continue;
        }
        if width >= max {
            cut = true;
            continue;
        }
        out.push(c);
        width += 1;
    }
    if cut {
        out.push_str("\x1b[0m");
    }
    out
}

// ── Terminal size ─────────────────────────────────────────────────────────────

/// (cols, rows). ioctl first (term_size), then crossterm, then $COLUMNS/$LINES,
/// then 80x24. Always clamped to sane bounds.
pub fn term_size() -> (usize, usize) {
    let env_dim = |k: &str| env_nonempty(k).and_then(|v| v.parse::<usize>().ok());
    let (mut w, mut h) = term_size::dimensions()
        .or_else(|| {
            crossterm::terminal::size()
                .ok()
                .map(|(w, h)| (w as usize, h as usize))
        })
        .unwrap_or((0, 0));
    if w == 0 {
        w = env_dim("COLUMNS").unwrap_or(80);
    }
    if h == 0 {
        h = env_dim("LINES").unwrap_or(24);
    }
    (w.clamp(20, 500), h.clamp(5, 300))
}

// ── Layout ────────────────────────────────────────────────────────────────────

/// Chart height that makes the whole frame fit in `rows`.
///
/// Frame = header(2) + [summary(4)] + blank(1)
///       + charts * (h + overhead) + (charts - 1) blank separators.
/// `overhead` = lines a chart uses beyond `h`: rasciichart emits `h + 1` plot
/// lines plus a label line => 2; the manual renderer (bandwidthmon2) => 0.
///
/// Returns (height, show_summary). Summary is dropped before charts shrink
/// below 2 rows; below that the hard clip in `Screen::draw` takes over.
pub fn fit_chart_height(rows: usize, want: usize, charts: usize, summary: bool, overhead: usize) -> (usize, bool) {
    let charts = charts.max(1);
    let try_fit = |with_summary: bool| -> isize {
        let fixed = 2 + if with_summary { 4 } else { 0 } + 1 + (charts - 1);
        let avail = rows as isize - fixed as isize;
        avail / charts as isize - overhead as isize
    };
    let want = want.max(1);
    if summary {
        let h = try_fit(true);
        if h >= 2 {
            return ((h as usize).min(want), true);
        }
    }
    let h = try_fit(false);
    ((h.max(2) as usize).min(want), false)
}

// ── Plain (non-TTY) output ────────────────────────────────────────────────────

/// One log-style line per sample, for `ssh host cmd` without a PTY, pipes, logs.
pub fn plain_line(iface: &str, secs: u64, dl: &str, ul: &str) -> String {
    format!("[{:>5}s] {}  DL {}  UL {}", secs, iface, dl.trim(), ul.trim())
}

/// Write a line; returns false when stdout is gone (EPIPE etc.) so callers
/// can stop instead of panicking inside println!.
pub fn emit_plain(line: &str) -> bool {
    let mut o = io::stdout().lock();
    writeln!(o, "{}", line).and_then(|_| o.flush()).is_ok()
}

// ── Full-screen guard ─────────────────────────────────────────────────────────

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Restore the terminal. Safe to call more than once.
fn restore() {
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let mut out = io::stdout();
        let _ = out.write_all(b"\x1b[?7h"); // auto-wrap back on
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

pub struct Screen {
    last_size: (usize, usize),
}

impl Screen {
    /// Enter raw mode + alternate screen. None => caller must use plain output
    /// (not a TTY, `--plain`, or raw mode unavailable, e.g. ssh without -t).
    pub fn try_enter(force_plain: bool) -> Option<Screen> {
        if force_plain || !io::stdout().is_terminal() {
            return None;
        }
        if enable_raw_mode().is_err() {
            return None;
        }
        let mut out = io::stdout();
        if execute!(out, EnterAlternateScreen, Hide).is_err() {
            let _ = disable_raw_mode();
            return None;
        }
        let _ = out.write_all(b"\x1b[?7l\x1b[2J\x1b[H"); // wrap off, clear, home
        let _ = out.flush();
        ACTIVE.store(true, Ordering::SeqCst);

        // release builds use panic = "abort": the Drop guard never runs on
        // panic, so restore from the hook instead.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            prev(info);
        }));

        Some(Screen { last_size: (0, 0) })
    }

    /// Draw one frame (lines separated by '\n') with a single write.
    pub fn draw(&mut self, frame: &str) -> io::Result<()> {
        let (cols, rows) = term_size();
        let ascii = !caps().utf8;
        let max_w = cols.saturating_sub(1).max(1);

        let mut buf = String::with_capacity(frame.len() + 64 * rows);
        if (cols, rows) != self.last_size {
            buf.push_str("\x1b[2J"); // size changed: wipe stale glyphs
            self.last_size = (cols, rows);
        }
        buf.push_str("\x1b[H");

        let lines: Vec<&str> = frame.lines().take(rows).collect();
        let last = lines.len().saturating_sub(1);
        for (i, line) in lines.iter().enumerate() {
            let folded;
            let line = if ascii {
                folded = fold_ascii(line);
                folded.as_str()
            } else {
                line
            };
            buf.push_str(&truncate_visible(line, max_w));
            buf.push_str("\x1b[K");
            if i < last {
                buf.push_str("\r\n"); // raw mode: bare '\n' has no CR
            }
        }
        buf.push_str("\x1b[J");

        let mut out = io::stdout().lock();
        out.write_all(buf.as_bytes())?;
        out.flush()
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        restore();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_ignores_escapes() {
        let s = "\x1b[1m\x1b[38;5;51mHelloWorld\x1b[0m";
        let t = truncate_visible(s, 5);
        assert!(t.starts_with("\x1b[1m\x1b[38;5;51mHello"));
        assert!(!t.contains("World"));
        assert!(t.ends_with("\x1b[0m"));
        assert_eq!(truncate_visible("abc", 10), "abc");
    }

    #[test]
    fn fold_is_ascii() {
        let f = fold_ascii("═══ ▼ 12.0│ ╭╮╰╯ ─");
        assert!(f.is_ascii());
    }

    #[test]
    fn fit_24_rows_both_charts() {
        // 2 + 1 + 1 + 2*(h+2) <= 24  =>  h = 8
        assert_eq!(fit_chart_height(24, 10, 2, false, 2), (8, false));
        // with summary: 2 + 4 + 1 + 1 + 2*(h+2) <= 24  =>  h = 6
        assert_eq!(fit_chart_height(24, 10, 2, true, 2), (6, true));
        // tall terminal: capped by request
        assert_eq!(fit_chart_height(60, 10, 2, true, 2), (10, true));
        // tiny terminal: summary dropped, floor 2
        assert_eq!(fit_chart_height(10, 10, 2, true, 2), (2, false));
        // manual renderer, no label line: 2 + 1 + 1 + 2*h <= 24 => h = 10
        assert_eq!(fit_chart_height(24, 12, 2, false, 0), (10, false));
    }
}
