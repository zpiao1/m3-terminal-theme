//! Golden-file guard for everything m3sync writes. The goldens were recorded
//! from the build that replaced the Python (checked against it by parity.py,
//! since removed), so a failure here means the output changed. If that was
//! intended, rerun with UPDATE_GOLDEN=1 and review the diff in git.

use super::*;
use crate::palette::from_source;
use material_colors::color::Rgb;

/// Wallpaper sources seen in the log, plus the fallback and a grey.
const SOURCES: [u32; 6] = [0x328D82, 0xCB2017, 0x2D76A5, 0xAE5F42, 0x4285F4, 0x808080];

fn check(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden").join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (UPDATE_GOLDEN=1 creates it)", path.display()));
    assert!(expected == actual, "{name} differs from its golden file; UPDATE_GOLDEN=1 to accept");
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("m3sync-golden-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn palettes() {
    for source in SOURCES {
        let pal = from_source(Rgb::from_u32(source));
        check(
            &format!("palette-{source:06X}.json"),
            &(to_json(&json!({"dark": pal.dark, "light": pal.light}), 2) + "\n"),
        );
    }
}

#[test]
fn statusline_powershell_and_cmd() {
    let pal = from_source(Rgb::from_u32(SOURCES[0]));
    let dir = scratch("files");
    let targets = Targets { out_dir: dir.clone(), terminal_settings: vec![] };
    for dark in [true, false] {
        let mode = if dark { "dark" } else { "light" };
        let header = format!("# header ({mode})");
        assert_eq!(targets.statusline(pal.mode(dark), &header), Ok(true));
        assert_eq!(targets.powershell(pal.mode(dark), &header), Ok(true));
        // Unchanged input must not touch the files (terminals reload on write).
        assert_eq!(targets.statusline(pal.mode(dark), &header), Ok(false));
        assert_eq!(targets.powershell(pal.mode(dark), &header), Ok(false));
        for file in ["statusline.json", "statusline.sh", "palette.ps1"] {
            check(&format!("{mode}-{file}"), &std::fs::read_to_string(dir.join(file)).unwrap());
        }
        check(&format!("{mode}-cmd-prompt.txt"), &cmd_prompt_text(pal.mode(dark)));
    }
    // Windows PowerShell 5.1 reads a BOM-less script as ANSI.
    assert!(std::fs::read(dir.join("palette.ps1")).unwrap().is_ascii());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn terminal_settings_merge() {
    let pal = from_source(Rgb::from_u32(SOURCES[0]));
    let dir = scratch("wt");
    let path = dir.join("settings.json");
    // A user's own settings, CRLF + BOM as WT writes them, with a stale
    // copy of our scheme to replace and someone else's to keep.
    let user = r##"{"copyOnSelect": true, "profiles": {"defaults": {"font": {"face": "Cascadia Code"}}, "list": [{"name": "PowerShell"}]},
"schemes": [{"name": "Campbell", "background": "#0C0C0C"}, {"name": "Material You Dark", "background": "#000000"}], "theme": "dark"}"##;
    std::fs::write(&path, format!("\u{FEFF}{}", user.replace('\n', "\r\n"))).unwrap();

    assert_eq!(terminal_settings(&pal, true, &path), Ok(true));
    let dark = std::fs::read_to_string(&path).unwrap();
    check("settings-dark.json", &dark);
    let v: Value = serde_json::from_str(&dark).unwrap();
    let names = |key: &str| {
        v[key].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    assert_eq!(names("schemes"), ["Campbell", "Material You Dark", "Material You Light"]);
    assert_eq!(names("themes"), ["Material You Dark", "Material You Light"]);
    assert_eq!(v["profiles"]["defaults"]["font"]["face"], "Cascadia Code");
    assert_eq!(v["copyOnSelect"], true);

    assert_eq!(terminal_settings(&pal, true, &path), Ok(false), "rewrite with no change");
    assert_eq!(terminal_settings(&pal, false, &path), Ok(true));
    let light: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(light["theme"], "Material You Light");
    assert_eq!(light["schemes"], v["schemes"], "both schemes are written in either mode");

    assert_eq!(terminal_settings(&pal, true, &dir.join("absent.json")), Ok(false), "no terminal installed");
    std::fs::write(&path, "[1]").unwrap();
    assert!(terminal_settings(&pal, true, &path).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mix_rounds_half_to_even() {
    assert_eq!(mix("#FFFFFF", "#000000", 0.5), "#808080"); // 127.5 -> 128
    assert_eq!(mix("#010101", "#000000", 0.5), "#000000"); // 0.5 -> 0
    assert_eq!(mix("#030303", "#000000", 0.5), "#020202"); // 1.5 -> 2
}
