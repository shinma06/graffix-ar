//! Fixed spray palette. Colors are stored per-stroke as a palette index so
//! serialized works stay small and the palette can evolve.

use crate::ffi::{ColorF, SprayColorF, SprayColorValueF};

/// Graffiti spray palette. A stroke's stored `palette_index` is the position
/// in this table, so entries may only be appended, never reordered.
pub const PALETTE: &[(SprayColorF, ColorF)] = &[
    (SprayColorF::White, ColorF { r: 245, g: 245, b: 245, a: 255 }),
    (SprayColorF::Black, ColorF { r: 20, g: 20, b: 22, a: 255 }),
    (SprayColorF::Silver, ColorF { r: 176, g: 180, b: 186, a: 255 }),
    (SprayColorF::Red, ColorF { r: 226, g: 35, b: 26, a: 255 }),
    (SprayColorF::Orange, ColorF { r: 245, g: 130, b: 32, a: 255 }),
    (SprayColorF::Yellow, ColorF { r: 250, g: 220, b: 40, a: 255 }),
    (SprayColorF::Green, ColorF { r: 40, g: 170, b: 80, a: 255 }),
    (SprayColorF::Teal, ColorF { r: 0, g: 160, b: 160, a: 255 }),
    (SprayColorF::Blue, ColorF { r: 30, g: 90, b: 220, a: 255 }),
    (SprayColorF::Purple, ColorF { r: 130, g: 60, b: 200, a: 255 }),
    (SprayColorF::Pink, ColorF { r: 240, g: 110, b: 170, a: 255 }),
    (SprayColorF::Brown, ColorF { r: 130, g: 85, b: 50, a: 255 }),
];

pub fn color(index: u8) -> ColorF {
    PALETTE
        .get(index as usize)
        .map(|(_, c)| *c)
        .unwrap_or(PALETTE[0].1)
}

/// Stored palette index of a spray color.
pub fn index_of(id: SprayColorF) -> u8 {
    PALETTE
        .iter()
        .position(|(c, _)| *c == id)
        .expect("every spray color is in the palette") as u8
}

/// Spray color of a stored palette index (out of range = the first color,
/// matching `color`).
pub fn id_of(index: u8) -> SprayColorF {
    PALETTE
        .get(index as usize)
        .map(|(c, _)| *c)
        .unwrap_or(PALETTE[0].0)
}

pub fn values() -> Vec<SprayColorValueF> {
    PALETTE
        .iter()
        .map(|&(id, rgba)| SprayColorValueF { id, rgba })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saved works store palette indices: index → color must never change.
    #[test]
    fn stored_indices_keep_their_colors() {
        let rgb: Vec<(u8, u8, u8)> = (0..PALETTE.len() as u8)
            .map(|i| {
                let c = color(i);
                (c.r, c.g, c.b)
            })
            .collect();
        assert_eq!(
            rgb,
            vec![
                (245, 245, 245),
                (20, 20, 22),
                (176, 180, 186),
                (226, 35, 26),
                (245, 130, 32),
                (250, 220, 40),
                (40, 170, 80),
                (0, 160, 160),
                (30, 90, 220),
                (130, 60, 200),
                (240, 110, 170),
                (130, 85, 50),
            ]
        );
        assert_eq!(color(200), color(0));
    }

    #[test]
    fn ids_round_trip_through_indices() {
        for (i, v) in values().iter().enumerate() {
            assert_eq!(index_of(v.id) as usize, i);
            assert_eq!(id_of(i as u8), v.id);
            assert_eq!(color(i as u8), v.rgba);
        }
        assert_eq!(id_of(200), SprayColorF::White);
    }
}
