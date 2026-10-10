//! Keep Windows Terminal, Intelligent Terminal, the Claude Code statusline and
//! the PowerShell/cmd prompts on a Material 3 Expressive palette derived from
//! the wallpaper.
//!
//!     m3sync            watch wallpaper + light/dark mode, re-theme on change
//!     m3sync --once     apply the current wallpaper once and exit
//!     m3sync --preview  print the palette without writing anything
//!     m3sync --log FILE append log lines to FILE
#![windows_subsystem = "windows"]

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

/// The cache holds only the expensive step's result - the wallpaper's source
/// colour (decode + quantize + score) - keyed by the file's mtime. Schemes are
/// rebuilt from it every time (a few ms), so editing a role table or updating
/// material-colors can't leave a stale palette behind. Bump this only if the
/// sampling or quantizing changes.
const CACHE_GENERATOR: &str = "m3sync-rs/source-v1";

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));

    if flag("--preview") {
        preview(&palette::from_source(palette::source_color(&palette::wallpaper_path())));
        return;
    }
    if flag("--help") || flag("-h") {
        println!("usage: m3sync [--once] [--preview] [--log FILE]");
        return;
    }

    let mut log = Log(value("--log").and_then(|p| File::options().append(true).create(true).open(p).ok()));
    let once = flag("--once");
    // Held for the process lifetime: a cheap cross-process lock.
    let _lock = if once {
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

    let mut sync = Sync::new(Targets::system());
    if once {
        sync.apply("startup", &mut log);
        return;
    }

    // Watch before the first apply, so a change while it settles and decodes
    // is not left waiting for the re-check.
    let (tx, rx) = mpsc::channel();
    let wallpaper_dir = palette::wallpaper_path().parent().map(Path::to_path_buf).unwrap_or_default();
    for started in [win::watch_directory(&wallpaper_dir, tx.clone()), win::watch_registry(win::PERSONALIZE_KEY, tx)] {
        if let Err(e) = started {
            log.line(&format!("watcher: {e}; relying on the {}s re-check", RECHECK.as_secs()));
        }
    }
    sync.apply("startup", &mut log);
    log.line("watching wallpaper and light/dark mode");
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

struct Sync {
    targets: Targets,
    /// The wallpaper's source colour and the mtime (ns) it was taken at. A
    /// light/dark flip only re-renders, and a login with an unchanged
    /// wallpaper skips the decode + quantize entirely.
    source: Option<(u64, Rgb)>,
    last: Option<(Option<u64>, bool)>,
}

impl Sync {
    fn new(targets: Targets) -> Self {
        let source = load_cache(&targets.cache_path());
        Self { targets, source, last: None }
    }

    fn apply(&mut self, reason: &str, log: &mut Log) {
        let (mut mtime, dark) = (wallpaper_mtime(), win::is_dark());
        if self.last == Some((mtime, dark)) {
            return;
        }
        let t0 = Instant::now();
        if mtime.is_none() || self.source.map(|s| s.0) != mtime {
            wait_for_settle(); // Windows writes the file in bursts
            mtime = wallpaper_mtime();
            let source = (mtime.unwrap_or(0), palette::source_color(&palette::wallpaper_path()));
            let cache = json!({"generator": CACHE_GENERATOR, "mtime_ns": source.0, "source": palette::hexcolor(source.1)});
            let _ = targets::write_if_changed(&self.targets.cache_path(), &(targets::to_json(&cache, 2) + "\n"));
            self.source = Some(source);
        }
        let pal = palette::from_source(self.source.expect("filled above").1);
        let changed = self.targets.apply(&pal, dark); // also creates the output dir
        self.last = Some((mtime, dark));
        log.line(&format!(
            "{reason:<9} source {} {}  {:.2}s  updated: {}",
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
        let fg = if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 > 128_000 { "0;0;0" } else { "255;255;255" };
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
