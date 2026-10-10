//! Turn the desktop wallpaper into a Material 3 Expressive palette.
//!
//! The pipeline is the one Android uses for dynamic colour, via material-colors
//! (a port of Google's material-color-utilities that tracks upstream):
//!
//!     wallpaper -> QuantizerCelebi (128 colours) -> Score -> source colour
//!               -> SchemeExpressive, 2025 spec, dark and light
//!
//! Terminals also need the 16 ANSI colours, which M3 doesn't define. They are
//! built the way M3 builds "custom colours": each keeps a fixed semantic hue
//! (red is still red) but harmonize rotates it up to 15 degrees toward the
//! source, then it is drawn at a tone chosen for the mode, so contrast against
//! the surface is guaranteed rather than hoped for. Which role each UI element
//! uses is decided in one place: targets.rs.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use material_colors::blend::harmonize;
use material_colors::color::Rgb;
use material_colors::dynamic_color::{DynamicScheme, Platform, SpecVersion};
use material_colors::hct::Hct;
use material_colors::palette::TonalPalette;
use material_colors::quantize::{Quantizer, QuantizerCelebi};
use material_colors::scheme::variant::SchemeExpressive;
use material_colors::score::Score;

/// What Android downsamples to; plenty for quantizing.
const SAMPLE_SIZE: u32 = 256;
const FALLBACK_SOURCE: Rgb = Rgb::from_u32(0x4285F4);

/// Role name -> '#RRGGBB', in a stable order (it is written out as JSON).
pub type Colors = IndexMap<String, String>;

pub struct Palette {
    pub source: String,
    pub dark: Colors,
    pub light: Colors,
}

impl Palette {
    pub fn mode(&self, dark: bool) -> &Colors {
        if dark { &self.dark } else { &self.light }
    }
}

type RoleFn = fn(&DynamicScheme) -> Rgb;

const ROLES: [(&str, RoleFn); 40] = [
    ("primary", DynamicScheme::primary),
    ("onPrimary", DynamicScheme::on_primary),
    ("primaryContainer", DynamicScheme::primary_container),
    ("onPrimaryContainer", DynamicScheme::on_primary_container),
    ("secondary", DynamicScheme::secondary),
    ("onSecondary", DynamicScheme::on_secondary),
    ("secondaryContainer", DynamicScheme::secondary_container),
    ("onSecondaryContainer", DynamicScheme::on_secondary_container),
    ("tertiary", DynamicScheme::tertiary),
    ("onTertiary", DynamicScheme::on_tertiary),
    ("tertiaryContainer", DynamicScheme::tertiary_container),
    ("onTertiaryContainer", DynamicScheme::on_tertiary_container),
    ("error", DynamicScheme::error),
    ("onError", DynamicScheme::on_error),
    ("errorContainer", DynamicScheme::error_container),
    ("onErrorContainer", DynamicScheme::on_error_container),
    ("primaryFixed", DynamicScheme::primary_fixed),
    ("primaryFixedDim", DynamicScheme::primary_fixed_dim),
    ("onPrimaryFixed", DynamicScheme::on_primary_fixed),
    ("secondaryFixed", DynamicScheme::secondary_fixed),
    ("secondaryFixedDim", DynamicScheme::secondary_fixed_dim),
    ("onSecondaryFixed", DynamicScheme::on_secondary_fixed),
    ("tertiaryFixed", DynamicScheme::tertiary_fixed),
    ("tertiaryFixedDim", DynamicScheme::tertiary_fixed_dim),
    ("onTertiaryFixed", DynamicScheme::on_tertiary_fixed),
    ("surface", DynamicScheme::surface),
    ("surfaceDim", DynamicScheme::surface_dim),
    ("surfaceBright", DynamicScheme::surface_bright),
    ("surfaceContainerLowest", DynamicScheme::surface_container_lowest),
    ("surfaceContainerLow", DynamicScheme::surface_container_low),
    ("surfaceContainer", DynamicScheme::surface_container),
    ("surfaceContainerHigh", DynamicScheme::surface_container_high),
    ("surfaceContainerHighest", DynamicScheme::surface_container_highest),
    ("onSurface", DynamicScheme::on_surface),
    ("onSurfaceVariant", DynamicScheme::on_surface_variant),
    ("outline", DynamicScheme::outline),
    ("outlineVariant", DynamicScheme::outline_variant),
    ("inverseSurface", DynamicScheme::inverse_surface),
    ("inverseOnSurface", DynamicScheme::inverse_on_surface),
    ("inversePrimary", DynamicScheme::inverse_primary),
];

/// Semantic anchors for the harmonized custom colours (HCT hue, chroma).
pub const CUSTOM: [(&str, f64, f64); 6] = [
    ("red", 25.0, 72.0),
    ("green", 142.0, 56.0),
    ("yellow", 86.0, 64.0),
    ("blue", 262.0, 56.0),
    ("magenta", 330.0, 56.0),
    ("cyan", 200.0, 48.0),
];

/// ANSI tones per mode: (normal, bright).
const fn ansi_tones(dark: bool) -> (i32, i32) {
    if dark { (78, 88) } else { (42, 32) }
}

/// M3 "Fixed" tones are the same in both modes, which is what a pill sitting
/// on the terminal background needs; containers flip with the mode.
const FIXED_TONES: (i32, i32) = (90, 10);

pub fn wallpaper_path() -> PathBuf {
    let appdata = std::env::var_os("APPDATA").unwrap_or_default();
    Path::new(&appdata).join(r"Microsoft\Windows\Themes\TranscodedWallpaper")
}

/// The top-scoring colour in the wallpaper.
pub fn source_color(path: &Path) -> Rgb {
    let Some(pixels) = sample(path) else {
        return FALLBACK_SOURCE;
    };
    let quantized = QuantizerCelebi::quantize(&pixels, 128);
    Score::score(&quantized.color_to_count, None, Some(FALLBACK_SOURCE), None)
        .first()
        .copied()
        .unwrap_or(FALLBACK_SOURCE)
}

fn sample(path: &Path) -> Option<Vec<Rgb>> {
    // TranscodedWallpaper has no extension; sniff the format from the bytes.
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let small = img.thumbnail(SAMPLE_SIZE, SAMPLE_SIZE).to_rgb8();
    Some(small.pixels().map(|p| Rgb::new(p[0], p[1], p[2])).collect())
}

pub fn hexcolor(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.red, c.green, c.blue)
}

/// '#RRGGBB' -> (r, g, b).
pub fn rgb(hex: &str) -> [u8; 3] {
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    [channel(1), channel(3), channel(5)]
}

/// Semantic hue -> TonalPalette, harmonized toward the source (mode-independent).
fn custom_palettes(source: Rgb) -> Vec<(&'static str, TonalPalette)> {
    CUSTOM
        .iter()
        .map(|&(name, hue, chroma)| {
            let anchor: Rgb = Hct::from(hue, chroma, 50.0).into();
            (name, TonalPalette::from_hue_and_chroma(Hct::new(harmonize(anchor, source)).get_hue(), chroma))
        })
        .collect()
}

fn scheme(source: Rgb, dark: bool, customs: &[(&str, TonalPalette)]) -> Colors {
    let s = SchemeExpressive::with_spec(Hct::new(source), dark, Some(0.0), SpecVersion::Spec2025, Platform::Phone).scheme;
    let mut colors: Colors = ROLES.iter().map(|(role, get)| (role.to_string(), hexcolor(get(&s)))).collect();
    let (normal, bright) = ansi_tones(dark);
    let (fixed, on_fixed) = FIXED_TONES;
    for (name, pal) in customs {
        let cap = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
        colors.insert(name.to_string(), hexcolor(pal.tone(normal)));
        colors.insert(format!("{name}Bright"), hexcolor(pal.tone(bright)));
        colors.insert(format!("{name}Fixed"), hexcolor(pal.tone(fixed)));
        colors.insert(format!("on{cap}Fixed"), hexcolor(pal.tone(on_fixed)));
    }
    colors
}

pub fn from_source(source: Rgb) -> Palette {
    let customs = custom_palettes(source);
    Palette {
        source: hexcolor(source),
        dark: scheme(source, true, &customs),
        light: scheme(source, false, &customs),
    }
}

pub fn build(path: &Path) -> Palette {
    from_source(source_color(path))
}
