//! T3 Code Material 3 Expressive (2026 spec) theme target.
//!
//! Writes `~/.t3/userdata/themes/material-3-expressive.json` with dual dark and
//! light role palettes plus Expressive shape/spring-motion CSS when T3 Code's
//! themes directory exists. T3 Code's directory watcher hot-reloads on change.

use std::fmt::Write;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::palette::{Colors, Palette};

use super::{Result, mix, to_json, write_if_changed};

pub const THEME_ID: &str = "material-3-expressive";
pub const THEME_NAME: &str = "Material 3 Expressive";

const EXPRESSIVE_CSS: &str = include_str!("t3_expressive.css");

const CSS_VARS: [(&str, &str); 23] = [
    ("--success", "green"),
    ("--success-foreground", "greenBright"),
    ("--info", "blue"),
    ("--info-foreground", "blueBright"),
    ("--guest", "tertiary"),
    ("--guest-foreground", "tertiary"),
    ("--chart-1", "primary"),
    ("--chart-2", "tertiary"),
    ("--chart-3", "secondary"),
    ("--chart-4", "green"),
    ("--chart-5", "yellow"),
    ("--chart-6", "cyan"),
    ("--m3-primary", "primary"),
    ("--m3-on-primary", "onPrimary"),
    ("--m3-primary-container", "primaryContainer"),
    ("--m3-on-primary-container", "onPrimaryContainer"),
    ("--m3-secondary-container", "secondaryContainer"),
    ("--m3-on-secondary-container", "onSecondaryContainer"),
    ("--m3-tertiary", "tertiary"),
    ("--m3-surface-container", "surfaceContainer"),
    ("--m3-surface-container-high", "surfaceContainerHigh"),
    ("--m3-outline", "outline"),
    ("--m3-outline-variant", "outlineVariant"),
];

fn roles(c: &Colors, dark: bool) -> Map<String, Value> {
    let entries = [
        ("canvas", c["surface"].clone()),
        ("chrome", c["surfaceContainerLow"].clone()),
        ("toolbar", c["surface"].clone()),
        ("toolbarForeground", c["onSurface"].clone()),
        ("toolbarBorder", c["outlineVariant"].clone()),
        ("toolbarControl", c["surfaceContainerHigh"].clone()),
        ("toolbarControlForeground", c["onSurface"].clone()),
        ("toolbarControlHover", c["surfaceContainerHighest"].clone()),
        ("surface", c["surfaceContainer"].clone()),
        ("surfaceRaised", c["surfaceContainerHigh"].clone()),
        ("surfaceOverlay", c["surfaceContainerHighest"].clone()),
        ("text", c["onSurface"].clone()),
        ("textMuted", c["onSurfaceVariant"].clone()),
        ("border", c["outlineVariant"].clone()),
        ("input", c["outline"].clone()),
        ("focus", c["primary"].clone()),
        ("accent", c["primary"].clone()),
        ("accentForeground", c["onPrimary"].clone()),
        ("secondary", c["secondaryContainer"].clone()),
        ("secondaryForeground", c["onSecondaryContainer"].clone()),
        ("muted", c["surfaceContainerHigh"].clone()),
        ("mutedForeground", c["onSurfaceVariant"].clone()),
        ("placeholder", mix(&c["onSurfaceVariant"], &c["outline"], 0.65)),
        ("secondaryLabel", c["onSurfaceVariant"].clone()),
        ("iconMuted", c["onSurfaceVariant"].clone()),
        ("error", c["error"].clone()),
        ("errorForeground", c[if dark { "onErrorContainer" } else { "error" }].clone()),
        ("errorSurface", mix(&c["error"], &c["surface"], if dark { 0.18 } else { 0.10 })),
        ("warning", c["yellow"].clone()),
        ("warningForeground", c["yellowBright"].clone()),
        ("warningSurface", mix(&c["yellow"], &c["surface"], if dark { 0.16 } else { 0.12 })),
        ("update", c["tertiary"].clone()),
        ("updateForeground", c["tertiary"].clone()),
        ("updateSurface", mix(&c["tertiary"], &c["surface"], if dark { 0.16 } else { 0.12 })),
        ("accentSurface", c["primaryContainer"].clone()),
        ("accentSurfaceForeground", c["onPrimaryContainer"].clone()),
        ("messageSurface", c["primaryContainer"].clone()),
        ("messageForeground", c["onPrimaryContainer"].clone()),
        ("messageAction", c["primary"].clone()),
        ("messageActionForeground", c["onPrimary"].clone()),
        ("messageActionHover", mix(&c["onPrimary"], &c["primary"], 0.08)),
        ("codeBackground", c[if dark { "surfaceContainerLow" } else { "surfaceContainerLowest" }].clone()),
        ("codeForeground", c["onSurface"].clone()),
        ("searchMatchBackground", c["secondaryContainer"].clone()),
        ("searchMatchForeground", c["onSecondaryContainer"].clone()),
        ("searchMatchActiveBackground", c["tertiaryFixed"].clone()),
        ("searchMatchActiveForeground", c["onTertiaryFixed"].clone()),
        ("sidebar", c["surfaceContainerLow"].clone()),
        ("sidebarForeground", c["onSurface"].clone()),
        ("sidebarMutedForeground", c["onSurfaceVariant"].clone()),
        ("sidebarControlSurface", c["surfaceContainerHigh"].clone()),
        ("sidebarRowHover", c["surfaceContainer"].clone()),
        ("sidebarRowActive", c["surfaceContainerHigh"].clone()),
        ("sidebarRowSelected", c["secondaryContainer"].clone()),
        ("sidebarBorder", c["outlineVariant"].clone()),
        ("terminalBackground", c["surface"].clone()),
        ("terminalForeground", c["onSurface"].clone()),
        ("terminalCursor", c["primary"].clone()),
        ("terminalSelection", c["secondaryContainer"].clone()),
        ("terminalScrollbar", c["outlineVariant"].clone()),
        ("terminalScrollbarHover", c["outline"].clone()),
    ];
    entries.into_iter().map(|(k, v)| (k.to_string(), Value::String(v))).collect()
}

fn format_vars(out: &mut String, selector: &str, c: &Colors) {
    let _ = writeln!(out, "{selector} {{");
    for (var, role) in CSS_VARS {
        let _ = writeln!(out, "  {var}: {};", c[role]);
    }
    out.push_str("}\n\n");
}

fn theme_css(pal: &Palette) -> String {
    let mut css = String::new();
    format_vars(&mut css, ":root:not(.dark)", &pal.light);
    // `:root.dark` matches Tailwind's `:root:is(.dark, .dark *)` (0,2,0) specificity in T3 Code.
    format_vars(&mut css, ":root.dark, .dark", &pal.dark);
    css.push_str(&EXPRESSIVE_CSS.replace("\r\n", "\n"));
    css
}

pub fn theme_json(pal: &Palette) -> Value {
    json!({
        "version": 1,
        "name": THEME_NAME,
        "appearance": "dark",
        "colors": roles(&pal.dark, true),
        "variants": {
            "light": roles(&pal.light, false),
        },
        "css": theme_css(pal),
    })
}

pub fn write_theme(pal: &Palette, themes_dir: &Path) -> Result<bool> {
    if !themes_dir.is_dir() {
        return Ok(false);
    }
    let path = themes_dir.join(format!("{THEME_ID}.json"));
    write_if_changed(&path, &(to_json(&theme_json(pal), 2) + "\n"))
}
