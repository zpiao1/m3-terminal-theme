//! Keep Windows Terminal, Intelligent Terminal, T3 Code, the Claude Code /
//! Antigravity CLI statuslines and the PowerShell/cmd prompts on a Material 3
//! Expressive palette derived from the Windows accent colour (or wallpaper).
//!
//!     m3sync                       watch Windows accent + light/dark mode, re-theme on change
//!     m3sync --source wallpaper    derive directly from TranscodedWallpaper instead
//!     m3sync --once                apply the current palette once and exit
//!     m3sync --preview             print the palette without writing anything
//!     m3sync --log FILE            append log lines to FILE
#![windows_subsystem = "windows"]
// Only win.rs may use unsafe (for Win32 calls), and every block there says why it is sound.
#![deny(unsafe_code)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod palette;
mod targets;
mod win;

use std::fs::File;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use material_colors::color::Rgb;
use palette::Palette;
use targets::Targets;

const SINGLE_INSTANCE_PORT: u16 = 49732; // tt-keyboard-sync holds 49731

// Windows writes TranscodedWallpaper in several bursts; wait for it to settle.
const SETTLE: Duration = Duration::from_millis(400);
const SETTLE_TIMEOUT: Duration = Duration::from_secs(5);
const RECHECK: Duration = Duration::from_secs(300); // safety net in case a notification is missed

const USAGE: &str = "usage: m3sync [--source accent|wallpaper] [--once] [--preview] [--log FILE]";

/// The cache holds only the expensive step's result - the wallpaper's source
/// colour (decode + quantize + score) - keyed by the file's mtime. Schemes are
/// rebuilt from it every time (a few ms), so editing a role table or updating
/// material-colors can't leave a stale palette behind. Bump this only if the
/// sampling or quantizing changes.
const CACHE_GENERATOR: &str = "m3sync-rs/source-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Accent,
    Wallpaper,
}

impl Source {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "accent" => Some(Self::Accent),
            "wallpaper" => Some(Self::Wallpaper),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Wallpaper => "wallpaper",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Args {
    source: Source,
    once: bool,
    preview: bool,
    help: bool,
    log: Option<String>,
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut parsed = Args { source: Source::Accent, once: false, preview: false, help: false, log: None };
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--source" => {
                let v = value("--source")?;
                parsed.source =
                    Source::parse(&v).ok_or_else(|| format!("--source must be accent or wallpaper, not {v}"))?;
            }
            "--log" => parsed.log = Some(value("--log")?),
            "--once" => parsed.once = true,
            "--preview" => parsed.preview = true,
            "--help" | "-h" => parsed.help = true,
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    Ok(parsed)
}

fn read_accent_color() -> Option<Rgb> {
    win::read_binary(win::ACCENT_KEY, palette::ACCENT_VALUE)
        .and_then(|bytes| palette::decode_accent_palette(&bytes, palette::ACCENT_INDEX))
}

struct Log(Option<File>);

impl Log {
    fn line(&mut self, msg: &str) {
        let line = format!("{}  {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
        match &mut self.0 {
            Some(f) => {
                let _ = writeln!(f, "{line}");
            }
            None => println!("{line}"),
        }
    }
}

fn main() {
    win::attach_parent_console();
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if args.help {
        println!("{USAGE}");
        return;
    }

    if args.preview {
        let source = match args.source {
            Source::Accent => read_accent_color().unwrap_or_else(|| {
                eprintln!(
                    "HKCU\\{}\\{} unreadable; showing the fallback colour",
                    win::ACCENT_KEY,
                    palette::ACCENT_VALUE
                );
                palette::FALLBACK_SOURCE
            }),
            Source::Wallpaper => palette::source_color(&palette::wallpaper_path()).unwrap_or_else(|e| {
                eprintln!("{e}; showing the fallback colour");
                palette::FALLBACK_SOURCE
            }),
        };
        preview(&palette::from_source(source));
        return;
    }

    let mut log = Log(args.log.as_deref().and_then(|p| File::options().append(true).create(true).open(p).ok()));
    // Held for the process lifetime: a cheap cross-process lock.
    let _lock = if args.once {
        None
    } else {
        match TcpListener::bind(("127.0.0.1", SINGLE_INSTANCE_PORT)) {
            Ok(l) => Some(l),
            Err(_) => {
                log.line("another instance is already running; exiting");
                return;
            }
        }
    };

    let mut sync = Sync::new(args.source, Targets::system());
    if args.once {
        sync.apply("startup", &mut log);
        return;
    }

    // Watch before the first apply, so a change while it settles and decodes
    // is not left waiting for the re-check.
    let (tx, rx) = mpsc::channel();
    let primary_watch = match args.source {
        Source::Accent => win::watch_registry(win::ACCENT_KEY, tx.clone()),
        Source::Wallpaper => {
            let wallpaper_dir = palette::wallpaper_path().parent().map(Path::to_path_buf).unwrap_or_default();
            win::watch_directory(&wallpaper_dir, tx.clone())
        }
    };
    for started in [primary_watch, win::watch_registry(win::PERSONALIZE_KEY, tx)] {
        if let Err(e) = started {
            log.line(&format!("watcher: {e}; relying on the {}s re-check", RECHECK.as_secs()));
        }
    }
    sync.apply("startup", &mut log);
    log.line(match args.source {
        Source::Accent => "watching Windows accent and light/dark mode",
        Source::Wallpaper => "watching wallpaper and light/dark mode",
    });
    loop {
        let reason = match rx.recv_timeout(RECHECK) {
            Ok(()) => "change",
            Err(RecvTimeoutError::Timeout) => "recheck",
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(RECHECK);
                "recheck"
            }
        };
        while rx.try_recv().is_ok() {} // one apply covers a burst of notifications
        sync.apply(reason, &mut log);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangeKey {
    Accent([u8; 3]),
    Wallpaper(Option<u64>),
}

struct Sync {
    source_kind: Source,
    targets: Targets,
    /// The wallpaper's source colour and the mtime (ns) it was taken at. A
    /// light/dark flip only re-renders, and a login with an unchanged
    /// wallpaper skips the decode + quantize entirely. A wallpaper that could
    /// not be read is shown as the fallback but never stored here.
    source: Option<(u64, Rgb)>,
    last: Option<(ChangeKey, bool)>,
}

impl Sync {
    fn new(source_kind: Source, targets: Targets) -> Self {
        let source = (source_kind == Source::Wallpaper).then(|| load_cache(&targets.cache_path())).flatten();
        Self { source_kind, targets, source, last: None }
    }

    fn apply(&mut self, reason: &str, log: &mut Log) {
        let dark = win::is_dark();
        let t0 = Instant::now();
        let (resolved, key) = match self.source_kind {
            Source::Accent => {
                let color = read_accent_color();
                if let Some(c) = color {
                    let k = ChangeKey::Accent([c.red, c.green, c.blue]);
                    if self.last == Some((k, dark)) {
                        return;
                    }
                    (Some(c), Some(k))
                } else {
                    log.line(&format!(
                        "accent unreadable (HKCU\\{}\\{}); using the fallback colour",
                        win::ACCENT_KEY,
                        palette::ACCENT_VALUE
                    ));
                    (None, None)
                }
            }
            Source::Wallpaper => {
                let mut mtime = wallpaper_mtime();
                if self.last == Some((ChangeKey::Wallpaper(mtime), dark)) {
                    return;
                }
                let mut source = self.source.filter(|s| Some(s.0) == mtime).map(|s| s.1);
                if source.is_none() {
                    wait_for_settle(); // Windows writes the file in bursts
                    mtime = wallpaper_mtime();
                    match palette::source_color(&palette::wallpaper_path()) {
                        Ok(color) => {
                            let at = mtime.unwrap_or(0);
                            let cache = json!({"generator": CACHE_GENERATOR, "mtime_ns": at, "source": palette::hexcolor(color)});
                            let _ = targets::write_if_changed(
                                &self.targets.cache_path(),
                                &(targets::to_json(&cache, 2) + "\n"),
                            );
                            self.source = Some((at, color));
                            source = Some(color);
                        }
                        // Possibly half-written: show the fallback, but leave `last`
                        // unset so the next notification or re-check reads it again.
                        Err(e) => log.line(&format!("wallpaper unreadable ({e}); using the fallback colour")),
                    }
                }
                (source, source.map(|_| ChangeKey::Wallpaper(mtime)))
            }
        };
        let pal = palette::from_source(resolved.unwrap_or(palette::FALLBACK_SOURCE));
        let changed = self.targets.apply(&pal, dark); // also creates the output dir
        self.last = key.map(|k| (k, dark));
        log.line(&format!(
            "{reason:<9} {} {} {}  {:.2}s  updated: {}",
            self.source_kind.name(),
            pal.source,
            if dark { "dark" } else { "light" },
            t0.elapsed().as_secs_f64(),
            if changed.is_empty() { "nothing".into() } else { changed.join(", ") }
        ));
    }
}

/// Nanoseconds since the epoch: an exact key, unlike float seconds.
fn wallpaper_mtime() -> Option<u64> {
    let modified = std::fs::metadata(palette::wallpaper_path()).ok()?.modified().ok()?;
    u64::try_from(modified.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_nanos()).ok()
}

fn wait_for_settle() {
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    let mut last = wallpaper_mtime();
    while Instant::now() < deadline {
        std::thread::sleep(SETTLE);
        let current = wallpaper_mtime();
        if current == last {
            return;
        }
        last = current;
    }
}

fn load_cache(path: &Path) -> Option<(u64, Rgb)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if v.get("generator")?.as_str()? != CACHE_GENERATOR {
        return None;
    }
    let [r, g, b] = palette::rgb(v.get("source")?.as_str()?);
    Some((v.get("mtime_ns")?.as_u64()?, Rgb::new(r, g, b)))
}

fn preview(pal: &Palette) {
    let swatch = |hex: &str, label: &str| {
        let [r, g, b] = palette::rgb(hex);
        let fg = if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 > 128_000 {
            "0;0;0"
        } else {
            "255;255;255"
        };
        format!("\x1b[48;2;{r};{g};{b}m\x1b[38;2;{fg}m {:<22}\x1b[0m", format!("{label} {hex}"))
    };
    println!("source {}", pal.source);
    for dark in [true, false] {
        println!("\n{}", if dark { "dark" } else { "light" });
        let mut roles: Vec<_> = pal.mode(dark).iter().collect();
        roles.sort();
        for (i, (role, hex)) in roles.iter().enumerate() {
            print!("{}{}", swatch(hex, role), if i % 4 == 3 { "\n" } else { "" });
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn parse_args_defaults_and_flags() {
        assert_eq!(
            parse(&[]),
            Ok(Args { source: Source::Accent, once: false, preview: false, help: false, log: None })
        );
        assert_eq!(
            parse(&["--source", "wallpaper", "--once", "--log", "out.log"]),
            Ok(Args {
                source: Source::Wallpaper,
                once: true,
                preview: false,
                help: false,
                log: Some("out.log".into()),
            })
        );
        assert_eq!(
            parse(&["--source", "accent", "--preview"]),
            Ok(Args { source: Source::Accent, once: false, preview: true, help: false, log: None })
        );
        assert!(parse(&["--source", "foo"]).is_err());
        assert!(parse(&["--source"]).is_err());
        assert!(parse(&["--log"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }
}
