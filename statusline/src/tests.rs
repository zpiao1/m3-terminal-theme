//! Unit tests for the pure parts. Byte-for-byte agreement with
//! statusline-command.sh is parity.sh's job; these pin the edge cases so
//! `cargo test` catches a regression without bash.

use super::*;
use serde_json::json;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("m3-statusline-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn grey() -> Palette {
    load_palette(Path::new("no such palette"))
}

#[test]
fn msys_and_native_paths_round_trip() {
    assert_eq!(native_path("/c/Users/x"), PathBuf::from("C:/Users/x"));
    assert_eq!(native_path("/d"), PathBuf::from("D:/"));
    assert_eq!(native_path("/cx/y"), PathBuf::from("/cx/y"), "not a drive");
    assert_eq!(native_path(r"C:\a"), PathBuf::from(r"C:\a"));
    assert_eq!(msys_path(Path::new(r"C:\Users\x")), "/c/Users/x");
    assert_eq!(msys_path(Path::new("/already/msys")), "/already/msys");
}

#[test]
fn home_becomes_tilde() {
    let home = Path::new(r"C:\Users\x");
    assert_eq!(tilde("/c/Users/x/repo", home), "~/repo");
    assert_eq!(tilde("/c/Users/x", home), "~");
    assert_eq!(tilde("/d/elsewhere", home), "/d/elsewhere");
    assert_eq!(tilde("/c/Users/x", Path::new("")), "/c/Users/x", "unknown home");
}

#[test]
fn long_paths_and_names_are_shortened() {
    assert_eq!(shorten_dir("~/a/b"), "~/a/b");
    assert_eq!(shorten_dir("~/a/b/c"), "~/a/b/c", "4 components stay");
    assert_eq!(shorten_dir("~/a/b/c/d"), "~/a/…/d");
    assert_eq!(shorten_dir("~"), "~");
    let twenty = "abcdefghijklmnopqrst";
    assert_eq!(cap(twenty), twenty);
    assert_eq!(cap(&format!("{twenty}u")), "abcdefghijklmnopqrs…");
    assert_eq!(cap("ünïcödé-földer-nämé-lång").chars().count(), MAX_NAME, "counts chars, not bytes");
    assert_eq!(shorten_dir(&format!("~/{twenty}uvw")), "~/abcdefghijklmnopqrs…");
}

#[test]
fn percentages_round_half_to_even_like_printf() {
    assert_eq!(round_pct(12.5), ("12".into(), 12));
    assert_eq!(round_pct(13.5), ("14".into(), 14));
    assert_eq!(round_pct(-3.0), ("-3".into(), -3));
}

#[test]
fn state_thresholds_are_inclusive() {
    assert_eq!(pick_state(49, 50, 75, "CTX"), "CTX");
    assert_eq!(pick_state(50, 50, 75, "CTX"), "WARN");
    assert_eq!(pick_state(75, 50, 75, "CTX"), "DANGER");
}

#[test]
fn palette_values_must_be_plain_rgb() {
    assert!(is_rgb("0;128;255"));
    for bad in ["1;2", "1;2;3;4", "1234;0;0", "1;2;3\x1b[5m", "a;b;c", ";;"] {
        assert!(!is_rgb(bad), "{bad:?}");
    }
    // An escape sequence smuggled into the palette keeps that pill grey.
    let dir = scratch("palette");
    let path = dir.join("statusline.json");
    let pill = json!({"bg": "1;2;3", "fg": "4;5;6", "track": "7;8;9"});
    let evil = json!({"bg": "1;2;3m\u{1b}]0;pwned\u{7}", "fg": "4;5;6", "track": "7;8;9"});
    std::fs::write(&path, json!({"pills": {"DIR": pill, "MODEL": evil}, "sep": "9;9;9"}).to_string()).unwrap();
    let p = load_palette(&path);
    assert_eq!((p.pill("DIR").bg.as_str(), p.sep.as_str()), ("1;2;3", "9;9;9"));
    assert_eq!(p.pill("MODEL").bg, "68;68;68");
    let _ = std::fs::remove_dir_all(&dir);
}

fn git_dir(name: &str, head: &str, refs: &[&str], packed: &[&str]) -> PathBuf {
    let git = scratch(name).join(".git");
    std::fs::create_dir_all(git.join("refs/heads")).unwrap();
    std::fs::write(git.join("HEAD"), head).unwrap();
    for r in refs {
        let path = git.join(r);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "0000000000000000000000000000000000000000\n").unwrap();
    }
    let lines: String = packed.iter().map(|r| format!("0000000000000000000000000000000000000000 {r}\n")).collect();
    std::fs::write(git.join("packed-refs"), format!("# pack-refs with: peeled fully-peeled sorted\n{lines}")).unwrap();
    git
}

fn branch(git: &Path) -> Option<String> {
    match branch_from_disk(git) {
        Disk::Branch(b) => Some(b),
        Disk::NoBranch => None,
        Disk::AskGit => Some("<ask git>".into()),
    }
}

#[test]
fn branch_is_read_from_disk() {
    let loose = git_dir("loose", "ref: refs/heads/feature/x\n", &["refs/heads/feature/x"], &[]);
    assert_eq!(branch(&loose).as_deref(), Some("feature/x"));
    let packed = git_dir("packed", "ref: refs/heads/main\n", &[], &["refs/heads/main"]);
    assert_eq!(branch(&packed).as_deref(), Some("main"));
    let unborn = git_dir("unborn", "ref: refs/heads/main\n", &[], &[]);
    assert_eq!(branch(&unborn), None);
    let detached = git_dir("detached", "0000000000000000000000000000000000000000\n", &[], &[]);
    assert_eq!(branch(&detached), None);
    // Windows can't even name a file like this ref, but packed-refs can list it.
    let escape = git_dir("escape", "ref: refs/heads/x\u{1b}[2J\n", &[], &["refs/heads/x\u{1b}[2J"]);
    assert_eq!(branch(&escape), None, "control characters never reach the terminal");
    let other = git_dir("other", "ref: refs/remotes/origin/main\n", &[], &[]);
    assert_eq!(branch(&other).as_deref(), Some("<ask git>"));
    let reftable = git_dir("reftable", "ref: refs/heads/.invalid\n", &[], &[]);
    std::fs::create_dir_all(reftable.join("reftable")).unwrap();
    assert_eq!(branch(&reftable).as_deref(), Some("<ask git>"));
    for git in [loose, packed, unborn, detached, escape, other, reftable] {
        let _ = std::fs::remove_dir_all(git.parent().unwrap());
    }
}

#[test]
fn worktree_git_file_points_at_its_git_dir() {
    let root = scratch("worktree");
    let target = root.join("main.git").join("worktrees").join("wt");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(root.join("wt").join("sub")).unwrap();
    std::fs::write(root.join("wt").join(".git"), format!("gitdir: {}\n", target.display())).unwrap();
    assert_eq!(find_git_dir(&root.join("wt").join("sub")), Some(target));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn render_minimal_and_full() {
    let pal = grey();
    let home = Path::new(r"C:\Users\x");
    let empty = render(&json!({}), home, &pal);
    assert!(empty.contains(" ~ ") && empty.starts_with(RST) && empty.ends_with(&format!("{ROUND_R}{RST}")));

    let all = json!({
        "cwd": "/c/Users/x", "model": {"id": "claude-opus-5-5[1m]", "display_name": "Opus 5.5"},
        "effort": {"level": "high"}, "cost": {"total_cost_usd": 7.5},
        "context_window": {"used_percentage": 62}, "rate_limits": {"five_hour": {"used_percentage": 81}}
    });
    let out = render(&all, home, &pal);
    for part in ["Opus 5.5·1M · high", " $7.50 ", " 62%", " 5h: 81% "] {
        assert!(out.contains(part), "{part:?} missing from {out:?}");
    }
    // 62% = 12 of 20 half-cells: 6 full in ink, 4 in the track tint.
    assert!(out.contains(&format!("━━━━━━{ESC}[38;2;115;115;115m━━━━{ESC}")), "{out:?}");
    // Same grey on every pill, so neighbours are joined by the thin separator.
    assert!(out.contains(SEP_THIN) && !out.contains(ARROW));
}
