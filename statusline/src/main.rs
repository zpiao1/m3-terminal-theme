//! Claude Code status line — Material 3 Expressive (wallpaper dynamic colour),
//! powerline. A port of ../statusline-command.sh that renders byte-identical
//! output without forking bash, jq or git on every refresh.
//!
//! Reads the status JSON on stdin, the palette m3sync.py writes to
//! ~/.config/m3-theme/statusline.json, and the branch straight from .git/HEAD.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

const ESC: &str = "\x1b";
const RST: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

// Powerline glyphs
const ARROW: &str = "\u{E0B0}"; // solid right arrow
const ROUND_L: &str = "\u{E0B6}"; // left half-circle
const ROUND_R: &str = "\u{E0B4}"; // right half-circle
const SEP_THIN: &str = "\u{E0B1}"; // thin chevron

/// The keys of targets.PILLS; a new pill there needs a segment here too.
const SEGMENTS: [&str; 9] = ["DIR", "BRANCH", "MODEL", "COST", "CTX", "5H", "7D", "WARN", "DANGER"];

fn main() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    // Unparseable input behaves like the script's failed jq pass: every field empty.
    let json: Value = serde_json::from_str(&input).unwrap_or(Value::Null);

    let home = home_dir();
    let palette = load_palette(&home.join(".config").join("m3-theme").join("statusline.json"));
    let out = render(&json, &home, &palette);
    let _ = std::io::stdout().lock().write_all(out.as_bytes());
}

/// Native home directory. Under Git Bash, MSYS hands native children HOME
/// already converted to C:\Users\...; USERPROFILE covers a launch without bash.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════════
// Palette: per segment the pill colour, its text colour and the
// progress-track tint, as "r;g;b". Written by targets.statusline().
// ═══════════════════════════════════════════════════════════════════════════

struct Pill {
    bg: String,
    fg: String,
    track: String,
}

struct Palette {
    pills: HashMap<&'static str, Pill>,
    sep: String,
}

impl Palette {
    fn pill(&self, seg: &str) -> &Pill {
        &self.pills[seg]
    }
}

fn load_palette(path: &Path) -> Palette {
    // Without the file every pill falls back to one neutral grey pair.
    let mut p = Palette {
        pills: SEGMENTS
            .iter()
            .map(|&s| (s, Pill { bg: "68;68;68".into(), fg: "230;230;230".into(), track: "115;115;115".into() }))
            .collect(),
        sep: "150;150;150".into(),
    };
    let Some(json) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return p;
    };
    // The values land inside escape sequences, so only accept the r;g;b shape
    // m3sync writes - anything else keeps the fallback.
    let rgb = |v: &Value| v.as_str().filter(|s| is_rgb(s)).map(str::to_string);
    for (seg, pill) in p.pills.iter_mut() {
        let Some(src) = json["pills"].get(*seg) else { continue };
        if let (Some(bg), Some(fg), Some(track)) = (rgb(&src["bg"]), rgb(&src["fg"]), rgb(&src["track"])) {
            *pill = Pill { bg, fg, track };
        }
    }
    if let Some(sep) = rgb(&json["sep"]) {
        p.sep = sep;
    }
    p
}

fn is_rgb(v: &str) -> bool {
    let parts: Vec<&str> = v.split(';').collect();
    parts.len() == 3
        && parts.iter().all(|p| (1..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Segments
// ═══════════════════════════════════════════════════════════════════════════

/// A string field as the script's jq pass yields it: null/false/"" are absent.
fn text<'a>(json: &'a Value, path: &[&str]) -> Option<&'a str> {
    lookup(json, path)?.as_str().filter(|s| !s.is_empty())
}

fn number(json: &Value, path: &[&str]) -> Option<f64> {
    lookup(json, path)?.as_f64()
}

fn lookup<'a>(json: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(json, |v, k| v.get(k))
}

/// Normal / warning / danger: M3 signals state by swapping the whole pill,
/// not just the text. Returns the segment whose C_/F_/T_ colours apply.
fn pick_state(value: i64, warn_at: i64, danger_at: i64, normal: &'static str) -> &'static str {
    if value >= danger_at {
        "DANGER"
    } else if value >= warn_at {
        "WARN"
    } else {
        normal
    }
}

/// printf '%.0f' and the integer bash then does arithmetic on. Both round half
/// to even, so 12.5 -> 12 here as in the script.
fn round_pct(v: f64) -> (String, i64) {
    (format!("{v:.0}"), v.round_ties_even() as i64)
}

fn render(json: &Value, home: &Path, pal: &Palette) -> String {
    let mut segments: Vec<(&str, String)> = Vec::new();

    // 1. Directory
    let cwd = text(json, &["cwd"]);
    let display_cwd = match cwd {
        Some(cwd) => shorten_dir(&tilde(cwd, home)),
        None => "~".to_string(),
    };
    segments.push(("DIR", format!(" {display_cwd} ")));

    // 2. Git branch
    if let Some(cwd) = cwd {
        let dir = native_path(cwd);
        if dir.is_dir() {
            if let Some(branch) = git_branch(&dir).filter(|b| !b.is_empty() && b != "HEAD") {
                segments.push(("BRANCH", format!("  {branch} ")));
            }
        }
    }

    // 3. Model + effort - display_name already carries the version ("Opus 5.5")
    let mut model = text(json, &["model", "display_name"]).unwrap_or("").to_string();
    if text(json, &["model", "id"]).is_some_and(|id| id.contains("[1m]")) {
        model.push_str("·1M");
    }
    // Live session effort (reflects /effort changes); fall back to settings default
    let effort = text(json, &["effort", "level"])
        .map(str::to_string)
        .or_else(|| settings_effort(home));
    if let Some(effort) = effort {
        model = format!("{model} · {effort}");
    }
    if !model.is_empty() {
        segments.push(("MODEL", format!("  {model} ")));
    }

    // 4. Cost - session total reported by Claude Code; warn >$5, danger >$15
    if let Some(cost) = number(json, &["cost", "total_cost_usd"]) {
        let seg = pick_state((cost * 100.0).floor() as i64, 500, 1500, "COST");
        segments.push((seg, format!(" ${cost:.2} ")));
    }

    // 5. Context - progress bar; warn >50%, danger >75%
    if let Some(used) = number(json, &["context_window", "used_percentage"]) {
        let (shown, ctx) = round_pct(used);
        let seg = pick_state(ctx, 50, 75, "CTX");
        let Pill { fg: ink, track, .. } = pal.pill(seg);
        // M3 linear progress: heavy line for the indicator, the same line in
        // the track tint for the rest (╸ = half step). 10 cells x 2 halves.
        let halves = ctx * 20 / 100;
        let full = (halves / 2).max(0) as usize;
        let half = halves % 2 == 1;
        let rest = 10usize.saturating_sub(full + half as usize);
        let bar = format!(
            "{}{}{ESC}[38;2;{track}m{}",
            "━".repeat(full),
            if half { "╸" } else { "" },
            "━".repeat(rest)
        );
        segments.push((seg, format!(" {bar}{ESC}[38;2;{ink}m {shown}% ")));
    }

    // 6./7. Rate limits - warn >50%, danger >80%
    for (key, normal, label) in [("five_hour", "5H", "5h"), ("seven_day", "7D", "7d")] {
        if let Some(pct) = number(json, &["rate_limits", key, "used_percentage"]) {
            let (shown, n) = round_pct(pct);
            segments.push((pick_state(n, 50, 80, normal), format!(" {label}: {shown}% ")));
        }
    }

    // Render: rounded left cap → arrow transitions → rounded right cap
    let mut out = String::with_capacity(512);
    let mut prev_bg: Option<&str> = None;
    for (name, content) in &segments {
        let Pill { bg, fg, .. } = pal.pill(name);
        let bg = bg.as_str();
        match prev_bg {
            None => out.push_str(&format!("{RST}{ESC}[38;2;{bg}m{ROUND_L}{ESC}[48;2;{bg}m")),
            Some(p) if p == bg => {
                out.push_str(&format!("{RST}{ESC}[48;2;{bg}m{ESC}[38;2;{}m{SEP_THIN}", pal.sep))
            }
            Some(p) => out.push_str(&format!("{RST}{ESC}[38;2;{p}m{ESC}[48;2;{bg}m{ARROW}")),
        }
        out.push_str(&format!("{ESC}[38;2;{fg}m{BOLD}{content}{RST}"));
        prev_bg = Some(bg);
    }
    if let Some(p) = prev_bg {
        out.push_str(&format!("{RST}{ESC}[38;2;{p}m{ROUND_R}{RST}"));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// Paths
// ═══════════════════════════════════════════════════════════════════════════

/// Claude Code reports cwd MSYS-style (/c/Users/...). Native file APIs need
/// C:/Users/...; anything else is passed through unchanged.
fn native_path(p: &str) -> PathBuf {
    let b = p.as_bytes();
    if b.len() >= 2 && b[0] == b'/' && b[1].is_ascii_alphabetic() && (b.len() == 2 || b[2] == b'/') {
        let drive = (b[1] as char).to_ascii_uppercase();
        return PathBuf::from(format!("{drive}:/{}", p.get(3..).unwrap_or("")));
    }
    PathBuf::from(p)
}

/// The inverse, for matching HOME against cwd: C:\Users\x -> /c/Users/x.
fn msys_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return format!("/{}{}", (b[0] as char).to_ascii_lowercase(), &s[2..]);
    }
    s
}

/// ${cwd/#$HOME/~}
fn tilde(cwd: &str, home: &Path) -> String {
    let home = msys_path(home);
    match cwd.strip_prefix(home.as_str()) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => cwd.to_string(),
    }
}

/// More than 4 components: keep the first two and the last, e.g. ~/a/…/d.
fn shorten_dir(d: &str) -> String {
    if d.matches('/').count() + 1 > 4 {
        let mut parts = d.splitn(3, '/');
        let p0 = parts.next().unwrap_or("");
        let p1 = parts.next().unwrap_or("");
        let last = d.rsplit('/').next().unwrap_or("");
        return format!("{p0}/{p1}/…/{last}");
    }
    d.to_string()
}

fn settings_effort(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(".claude").join("settings.json")).ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;
    match json.get("effortLevel")? {
        Value::Null | Value::Bool(false) => None,
        Value::String(s) => Some(s.clone()).filter(|s| !s.is_empty()),
        other => Some(other.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Git branch: what `git rev-parse --abbrev-ref HEAD` prints, read from disk
// ═══════════════════════════════════════════════════════════════════════════

fn git_branch(cwd: &Path) -> Option<String> {
    let git_dir = find_git_dir(cwd)?;
    match branch_from_disk(&git_dir) {
        Disk::Branch(b) => Some(b),
        Disk::NoBranch => None,
        Disk::AskGit => git_rev_parse(cwd),
    }
}

enum Disk {
    Branch(String),
    NoBranch,
    /// A layout this reader does not handle (reftable, a HEAD pointing outside
    /// refs/heads/) - defer to git itself rather than guess.
    AskGit,
}

/// Walks up from cwd like git's discovery. `.git` is a directory, or for a
/// worktree/submodule a file holding `gitdir: <path>`.
fn find_git_dir(cwd: &Path) -> Option<PathBuf> {
    for dir in cwd.ancestors() {
        let dot = dir.join(".git");
        if dot.is_dir() {
            if dot.join("HEAD").is_file() {
                return Some(dot);
            }
        } else if dot.is_file() {
            let text = std::fs::read_to_string(&dot).ok()?;
            let target = text.trim_end().strip_prefix("gitdir: ")?;
            return Some(dir.join(native_path(target)));
        }
    }
    None
}

fn branch_from_disk(git_dir: &Path) -> Disk {
    // Linked worktrees keep HEAD in their own dir but refs in the common dir.
    let common = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(c) => git_dir.join(native_path(c.trim_end())),
        Err(_) => git_dir.to_path_buf(),
    };
    if common.join("reftable").exists() {
        return Disk::AskGit;
    }
    let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) else {
        return Disk::AskGit;
    };
    let Some(refname) = head.trim_end().strip_prefix("ref: ") else {
        return Disk::NoBranch; // detached: git prints "HEAD", the script hides it
    };
    let Some(branch) = refname.strip_prefix("refs/heads/") else {
        return Disk::AskGit;
    };
    // HEAD is read raw, so git's ref-name rules don't protect it: a .git dir
    // unpacked from an archive can hold any bytes, and an ESC here would reach
    // the terminal as an escape sequence. git rejects such a ref outright.
    if branch.is_empty() || branch.chars().any(char::is_control) {
        return Disk::NoBranch;
    }
    // Unborn branch (fresh `git init`): the ref doesn't exist yet and
    // rev-parse fails, so there is no branch to show.
    if common.join(refname).is_file() || packed_ref_exists(&common, refname) {
        Disk::Branch(branch.to_string())
    } else {
        Disk::NoBranch
    }
}

fn packed_ref_exists(common: &Path, refname: &str) -> bool {
    std::fs::read_to_string(common.join("packed-refs")).is_ok_and(|text| {
        text.lines()
            .any(|l| !l.starts_with('#') && !l.starts_with('^') && l.split_once(' ').is_some_and(|(_, r)| r == refname))
    })
}

fn git_rev_parse(cwd: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_string())
}
