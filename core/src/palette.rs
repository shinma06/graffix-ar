//! Fixed spray palette. Colors are stored per-stroke as a palette index so
//! serialized works stay small and the palette can evolve.

use crate::ffi::{ColorF, PaletteEntryF};

/// Graffiti spray palette (name shown in Japanese UI).
pub const PALETTE: &[(&str, ColorF)] = &[
    ("ホワイト", ColorF { r: 245, g: 245, b: 245, a: 255 }),
    ("ブラック", ColorF { r: 20, g: 20, b: 22, a: 255 }),
    ("シルバー", ColorF { r: 176, g: 180, b: 186, a: 255 }),
    ("レッド", ColorF { r: 226, g: 35, b: 26, a: 255 }),
    ("オレンジ", ColorF { r: 245, g: 130, b: 32, a: 255 }),
    ("イエロー", ColorF { r: 250, g: 220, b: 40, a: 255 }),
    ("グリーン", ColorF { r: 40, g: 170, b: 80, a: 255 }),
    ("ティール", ColorF { r: 0, g: 160, b: 160, a: 255 }),
    ("ブルー", ColorF { r: 30, g: 90, b: 220, a: 255 }),
    ("パープル", ColorF { r: 130, g: 60, b: 200, a: 255 }),
    ("ピンク", ColorF { r: 240, g: 110, b: 170, a: 255 }),
    ("ブラウン", ColorF { r: 130, g: 85, b: 50, a: 255 }),
];

pub fn color(index: u8) -> ColorF {
    PALETTE
        .get(index as usize)
        .map(|(_, c)| *c)
        .unwrap_or(PALETTE[0].1)
}

pub fn entries() -> Vec<PaletteEntryF> {
    PALETTE
        .iter()
        .map(|(name, color)| PaletteEntryF {
            name: (*name).to_string(),
            color: *color,
        })
        .collect()
}
