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
pub const FALLBACK_SOURCE: Rgb = Rgb::from_u32(0x4285F4);

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

/// The top-scoring colour in the wallpaper (FALLBACK_SOURCE if nothing in it
/// scores), or why the image could not be read.
pub fn source_color(path: &Path) -> Result<Rgb, String> {
    let pixels = sample(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let quantized = QuantizerCelebi::quantize(&pixels, 128);
    Ok(Score::score(&quantized.color_to_count, None, Some(FALLBACK_SOURCE), None)
        .first()
        .copied()
        .unwrap_or(FALLBACK_SOURCE))
}

fn sample(path: &Path) -> image::ImageResult<Vec<Rgb>> {
    // TranscodedWallpaper has no extension; sniff the format from the bytes.
    let img = image::ImageReader::open(path)?.with_guessed_format()?.decode()?;
    let small = img.thumbnail(SAMPLE_SIZE, SAMPLE_SIZE).to_rgb8();
    Ok(small.pixels().map(|p| Rgb::new(p[0], p[1], p[2])).collect())
}

pub fn hexcolor(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.red, c.green, c.blue)
}

/// '#RRGGBB' -> (r, g, b). Only ever given hexcolor() output.
pub fn rgb(hex: &str) -> [u8; 3] {
    let c: Rgb = hex.parse().unwrap_or_default();
    [c.red, c.green, c.blue]
}

/// "magenta" -> "Magenta".
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default()
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
    let s =
        SchemeExpressive::with_spec(Hct::new(source), dark, Some(0.0), SpecVersion::Spec2025, Platform::Phone).scheme;
    let mut colors: Colors = ROLES.iter().map(|(role, get)| (role.to_string(), hexcolor(get(&s)))).collect();
    let (normal, bright) = ansi_tones(dark);
    let (fixed, on_fixed) = FIXED_TONES;
    for (name, pal) in customs {
        let cap = capitalize(name);
        colors.insert(name.to_string(), hexcolor(pal.tone(normal)));
        colors.insert(format!("{name}Bright"), hexcolor(pal.tone(bright)));
        colors.insert(format!("{name}Fixed"), hexcolor(pal.tone(fixed)));
        colors.insert(format!("on{cap}Fixed"), hexcolor(pal.tone(on_fixed)));
    }
    colors
}

pub fn from_source(source: Rgb) -> Palette {
    let customs = custom_palettes(source);
    Palette { source: hexcolor(source), dark: scheme(source, true, &customs), light: scheme(source, false, &customs) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tt-keyboard-sync's source.rs has the same test with the same expected
    /// colour: the keyboard must pick exactly the terminal's source colour.
    #[test]
    fn source_color_matches_tt_keyboard_sync() {
        let path = std::env::temp_dir().join(format!("m3sync-source-{}.png", std::process::id()));
        image::RgbImage::from_fn(300, 200, |x, y| image::Rgb([(x * 255 / 299) as u8, (y * 255 / 199) as u8, 160]))
            .save(&path)
            .unwrap();
        let got = source_color(&path).map(hexcolor);
        let _ = std::fs::remove_file(&path);
        assert_eq!(got.as_deref(), Ok("#EB17A0"));
        assert!(source_color(Path::new("no such wallpaper")).is_err());
    }
}
