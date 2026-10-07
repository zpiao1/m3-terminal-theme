"""Turn the desktop wallpaper into a Material 3 Expressive palette.

The pipeline is the one Android uses for dynamic colour, via
materialyoucolor (a port of Google's material-color-utilities):

    wallpaper -> QuantizeCelebi (128 colours) -> Score -> source colour
              -> SchemeExpressive, 2025 spec, dark and light

Terminals also need the 16 ANSI colours, which M3 doesn't define. They are
built the way M3 builds "custom colours": each keeps a fixed semantic hue (red
is still red) but Blend.harmonize rotates it up to 15 degrees toward the
source, then it is drawn at a tone chosen for the mode, so contrast against
the surface is guaranteed rather than hoped for. Which role each UI element
uses is decided in one place: targets.py.
"""
import os

from materialyoucolor.blend.blend import Blend
from materialyoucolor.dynamiccolor.material_dynamic_colors import MaterialDynamicColors as MDC
from materialyoucolor.hct import Hct
from materialyoucolor.palettes.tonal_palette import TonalPalette
from materialyoucolor.quantize import QuantizeCelebi
from materialyoucolor.scheme.scheme_expressive import SchemeExpressive
from materialyoucolor.score.score import Score

WALLPAPER_PATH = os.path.join(
    os.environ["APPDATA"], "Microsoft", "Windows", "Themes", "TranscodedWallpaper"
)

SAMPLE_SIZE = (256, 256)   # what Android downsamples to; plenty for quantizing
FALLBACK_SOURCE = 0xFF4285F4

ROLES = [
    "primary", "onPrimary", "primaryContainer", "onPrimaryContainer",
    "secondary", "onSecondary", "secondaryContainer", "onSecondaryContainer",
    "tertiary", "onTertiary", "tertiaryContainer", "onTertiaryContainer",
    "error", "onError", "errorContainer", "onErrorContainer",
    "primaryFixed", "primaryFixedDim", "onPrimaryFixed",
    "secondaryFixed", "secondaryFixedDim", "onSecondaryFixed",
    "tertiaryFixed", "tertiaryFixedDim", "onTertiaryFixed",
    "surface", "surfaceDim", "surfaceBright", "surfaceContainerLowest",
    "surfaceContainerLow", "surfaceContainer", "surfaceContainerHigh",
    "surfaceContainerHighest", "onSurface", "onSurfaceVariant",
    "outline", "outlineVariant", "inverseSurface", "inverseOnSurface",
    "inversePrimary",
]

# Semantic anchors for the harmonized custom colours (HCT hue, chroma).
CUSTOM = {
    "red": (25.0, 72.0),
    "green": (142.0, 56.0),
    "yellow": (86.0, 64.0),
    "blue": (262.0, 56.0),
    "magenta": (330.0, 56.0),
    "cyan": (200.0, 48.0),
}

# ANSI tones per mode: (normal, bright).
ANSI_TONES = {True: (78, 88), False: (42, 32)}
# M3 "Fixed" tones are the same in both modes, which is what a pill sitting on
# the terminal background needs; containers flip with the mode.
FIXED_TONES = (90, 10)


def source_color(path=WALLPAPER_PATH):
    """ARGB int of the top-scoring colour in the wallpaper."""
    import numpy as np
    from PIL import Image
    try:
        with Image.open(path) as im:
            im.draft("RGB", SAMPLE_SIZE)     # JPEG: let libjpeg decode at 1/8 scale
            im = im.convert("RGB")
            im.thumbnail(SAMPLE_SIZE, Image.Resampling.BILINEAR)
            rgb = np.asarray(im).reshape(-1, 3)
    except OSError:
        return FALLBACK_SOURCE
    pixels = np.hstack([rgb, np.full((len(rgb), 1), 255, rgb.dtype)]).tolist()
    ranked = Score.score(QuantizeCelebi(pixels, 128))
    return ranked[0] if ranked else FALLBACK_SOURCE


def hexcolor(argb):
    return "#%06X" % (argb & 0xFFFFFF)


def rgb(hexcolor):
    """'#RRGGBB' -> (r, g, b)."""
    return tuple(int(hexcolor[i:i + 2], 16) for i in (1, 3, 5))


def _custom_palettes(source):
    """Semantic hue -> TonalPalette, harmonized toward the source (mode-independent)."""
    out = {}
    for name, (hue, chroma) in CUSTOM.items():
        anchor = Hct.from_hct(hue, chroma, 50).to_int()
        out[name] = TonalPalette.from_hue_and_chroma(
            Hct.from_int(Blend.harmonize(anchor, source)).hue, chroma)
    return out


def scheme(source, dark, customs):
    """Dict of role name -> '#RRGGBB' for one mode."""
    s = SchemeExpressive(Hct.from_int(source), dark, 0.0, spec_version="2025")
    colors = {role: hexcolor(getattr(MDC, role).get_argb(s)) for role in ROLES}
    normal, bright = ANSI_TONES[dark]
    fixed, on_fixed = FIXED_TONES
    for name, pal in customs.items():
        cap = name.capitalize()
        colors[name] = hexcolor(pal.tone(normal))
        colors[name + "Bright"] = hexcolor(pal.tone(bright))
        colors[name + "Fixed"] = hexcolor(pal.tone(fixed))
        colors["on" + cap + "Fixed"] = hexcolor(pal.tone(on_fixed))
    return colors


def build(path=WALLPAPER_PATH):
    src = source_color(path)
    customs = _custom_palettes(src)
    return {"source": hexcolor(src),
            "dark": scheme(src, True, customs),
            "light": scheme(src, False, customs)}
