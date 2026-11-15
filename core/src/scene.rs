//! Scene simulation: how a saved work would look on a different wall, under
//! different light, seen from a given distance. Everything is local and
//! procedural (no bundled photos, no network).
//!
//! Model (linear light): the wall's albedo comes from its material pattern
//! (optionally recolored); paint sits on top with the material's absorption
//! and the texture showing through; the result is lit by an ambient term plus
//! a key light whose falloff depends on the situation; a filmic exposure
//! curve maps it to sRGB.

use crate::canvas::{Dab, Drip};
use crate::ffi::{ColorF, PaletteEntryF};
use crate::math::Vec2;
use crate::raster::{linear_to_srgb, srgb_to_linear, PaintLayer, View};

/// Horizontal field of view of the simulated viewer (an iPhone wide camera
/// in landscape is ~65°; 60° keeps the framing slightly conservative).
pub const CAMERA_HFOV_DEG: f32 = 60.0;
pub const MIN_VIEW_DISTANCE_M: f32 = 0.5;
pub const MAX_VIEW_DISTANCE_M: f32 = 30.0;
pub const DEFAULT_VIEW_DISTANCE_M: f32 = 3.0;
/// Margin around the work for fit-to-work framing, fraction of its size.
const FIT_MARGIN: f32 = 0.15;
/// Smallest framed area, meters (keeps a single dab from filling the view).
const FIT_MIN_SIZE_M: f32 = 0.4;
/// Height of the night street light above the work's top edge, and its
/// distance out from the wall, meters.
const STREETLIGHT_ABOVE_M: f32 = 0.8;
const STREETLIGHT_OUT_M: f32 = 1.4;
const EXPOSURE: f32 = 1.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lighting {
    /// 晴れの昼: sun from above plus blue sky fill.
    Day,
    /// 曇り: flat, soft daylight.
    Overcast,
    /// 夕方: low warm sun raking from the left, dim violet fill.
    Dusk,
    /// 夜: dark ambient, one street light above the work.
    Night,
}

pub const LIGHTINGS: &[(Lighting, &str)] = &[
    (Lighting::Day, "昼（晴れ）"),
    (Lighting::Overcast, "昼（曇り）"),
    (Lighting::Dusk, "夕方"),
    (Lighting::Night, "夜（街灯）"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    /// 打ちっぱなしコンクリート: porous, grainy.
    Concrete,
    /// シャッター: corrugated painted steel, smooth and a little glossy.
    Shutter,
    /// レンガ: running-bond bricks with recessed mortar.
    Brick,
    /// 白いモルタル壁: smooth rendered wall.
    Plaster,
}

pub const MATERIALS: &[(Material, &str)] = &[
    (Material::Concrete, "コンクリート"),
    (Material::Shutter, "シャッター"),
    (Material::Brick, "レンガ"),
    (Material::Plaster, "モルタル（白壁）"),
];

/// Wall recolor options. Index 0 keeps the material's own color.
pub const WALL_COLORS: &[(&str, Option<ColorF>)] = &[
    ("素材の色", None),
    (
        "ホワイト",
        Some(ColorF {
            r: 236,
            g: 234,
            b: 228,
            a: 255,
        }),
    ),
    (
        "グレー",
        Some(ColorF {
            r: 128,
            g: 128,
            b: 126,
            a: 255,
        }),
    ),
    (
        "ブラック",
        Some(ColorF {
            r: 38,
            g: 38,
            b: 40,
            a: 255,
        }),
    ),
    (
        "ベージュ",
        Some(ColorF {
            r: 206,
            g: 190,
            b: 160,
            a: 255,
        }),
    ),
    (
        "ネイビー",
        Some(ColorF {
            r: 38,
            g: 52,
            b: 88,
            a: 255,
        }),
    ),
    (
        "ミント",
        Some(ColorF {
            r: 150,
            g: 200,
            b: 180,
            a: 255,
        }),
    ),
];

pub fn wall_color_entries() -> Vec<PaletteEntryF> {
    WALL_COLORS
        .iter()
        .map(|(name, color)| PaletteEntryF {
            name: (*name).to_string(),
            color: color.unwrap_or(ColorF {
                r: 150,
                g: 148,
                b: 143,
                a: 255,
            }),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub lighting: Lighting,
    pub material: Material,
    /// Index into `WALL_COLORS` (out of range = material color).
    pub wall_color: u8,
}

/// How the virtual camera frames the wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Framing {
    /// Fit the whole work with a margin (timelapse, thumbnails).
    FitWork,
    /// Head-on from `d` meters with `CAMERA_HFOV_DEG`: the work occupies the
    /// share of the view it would in person.
    Distance(f32),
}

/// Wall-local bounding box of a work's paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub fn of<'a>(
        dabs: impl Iterator<Item = &'a Dab>,
        drips: impl Iterator<Item = &'a Drip>,
    ) -> Option<Self> {
        let mut b: Option<Bounds> = None;
        let mut add = |lo: Vec2, hi: Vec2| {
            b = Some(match b {
                None => Bounds { min: lo, max: hi },
                Some(o) => Bounds {
                    min: Vec2::new(o.min.x.min(lo.x), o.min.y.min(lo.y)),
                    max: Vec2::new(o.max.x.max(hi.x), o.max.y.max(hi.y)),
                },
            })
        };
        for d in dabs {
            let r = d.radius_major.max(d.radius_minor);
            add(
                Vec2::new(d.uv.x - r, d.uv.y - r),
                Vec2::new(d.uv.x + r, d.uv.y + r),
            );
        }
        for d in drips {
            let w = d.width * 0.5;
            add(
                Vec2::new(d.uv.x - w, d.uv.y - d.len),
                Vec2::new(d.uv.x + w, d.uv.y + w),
            );
        }
        b
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }
}

/// Clamp a requested viewing distance into the supported range.
pub fn clamp_distance(d: f32) -> f32 {
    if d.is_finite() {
        d.clamp(MIN_VIEW_DISTANCE_M, MAX_VIEW_DISTANCE_M)
    } else {
        DEFAULT_VIEW_DISTANCE_M
    }
}

/// The camera view for a framing. Empty works frame 1 m around the origin.
pub fn view_for(bounds: Option<Bounds>, framing: Framing, width: u32, height: u32) -> View {
    let b = bounds.unwrap_or(Bounds {
        min: Vec2::new(-0.5, -0.5),
        max: Vec2::new(0.5, 0.5),
    });
    let m_per_px = match framing {
        Framing::FitWork => {
            let w = (b.width() * (1.0 + 2.0 * FIT_MARGIN)).max(FIT_MIN_SIZE_M);
            let h = (b.height() * (1.0 + 2.0 * FIT_MARGIN)).max(FIT_MIN_SIZE_M);
            (w / width as f32).max(h / height as f32)
        }
        Framing::Distance(d) => {
            let view_w = 2.0 * clamp_distance(d) * (CAMERA_HFOV_DEG.to_radians() * 0.5).tan();
            view_w / width as f32
        }
    };
    View::centered(b.center(), m_per_px, width, height)
}

/// Per-point wall surface properties.
struct Surface {
    albedo: [f32; 3],
    /// Fraction of sprayed paint that stays visible here (porosity, recesses).
    paint_hold: f32,
    /// Shading from relief (ribs, mortar joints), multiplies lighting.
    relief: f32,
    /// Specular glare weight (metal).
    sheen: f32,
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0x00FF_FFFF) as f32 / 0x00FF_FFFF as f32
}

/// Smooth value noise in 0..1 at `cell` meters per lattice cell.
fn value_noise(p: Vec2, cell: f32, seed: u32) -> f32 {
    let x = p.x / cell;
    let y = p.y / cell;
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

fn lin(c: ColorF) -> [f32; 3] {
    [
        srgb_to_linear(c.r as f32 / 255.0),
        srgb_to_linear(c.g as f32 / 255.0),
        srgb_to_linear(c.b as f32 / 255.0),
    ]
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

impl Material {
    fn natural(self) -> ColorF {
        match self {
            Material::Concrete => ColorF {
                r: 152,
                g: 150,
                b: 145,
                a: 255,
            },
            Material::Shutter => ColorF {
                r: 170,
                g: 175,
                b: 180,
                a: 255,
            },
            Material::Brick => ColorF {
                r: 150,
                g: 74,
                b: 52,
                a: 255,
            },
            Material::Plaster => ColorF {
                r: 226,
                g: 223,
                b: 214,
                a: 255,
            },
        }
    }

    fn surface(self, p: Vec2, base: [f32; 3]) -> Surface {
        match self {
            Material::Concrete => {
                let grain = value_noise(p, 0.004, 1) * 0.6 + value_noise(p, 0.03, 2) * 0.4;
                let blotch = value_noise(p, 0.35, 3);
                let k = 0.82 + 0.22 * grain + 0.12 * (blotch - 0.5);
                Surface {
                    albedo: scale(base, k),
                    // Paint sinks into the pores; peaks of the grain stay bare-ish.
                    paint_hold: 0.9 - 0.25 * (grain - 0.5).max(0.0),
                    relief: 1.0,
                    sheen: 0.0,
                }
            }
            Material::Shutter => {
                const PERIOD_M: f32 = 0.076;
                let phase = (p.y / PERIOD_M).fract();
                let rib = (phase * std::f32::consts::TAU).cos();
                let streak = value_noise(Vec2::new(p.x, p.y * 0.05), 0.2, 4);
                Surface {
                    albedo: scale(base, 0.95 + 0.08 * (streak - 0.5)),
                    paint_hold: 1.0,
                    relief: 0.84 + 0.16 * rib,
                    sheen: (rib.max(0.0)).powi(8) * 0.35,
                }
            }
            Material::Brick => {
                const BRICK_W: f32 = 0.215;
                const BRICK_H: f32 = 0.065;
                const MORTAR: f32 = 0.01;
                let row_h = BRICK_H + MORTAR;
                let row = (p.y / row_h).floor();
                let offset = if (row as i32).rem_euclid(2) == 1 {
                    (BRICK_W + MORTAR) * 0.5
                } else {
                    0.0
                };
                let col_w = BRICK_W + MORTAR;
                let col = ((p.x + offset) / col_w).floor();
                let in_row = p.y - row * row_h;
                let in_col = p.x + offset - col * col_w;
                let mortar = in_row < MORTAR || in_col < MORTAR;
                let grain = value_noise(p, 0.006, 5);
                if mortar {
                    Surface {
                        albedo: scale(
                            lin(ColorF {
                                r: 172,
                                g: 166,
                                b: 156,
                                a: 255,
                            }),
                            0.9 + 0.15 * grain,
                        ),
                        paint_hold: 0.6,
                        relief: 0.8,
                        sheen: 0.0,
                    }
                } else {
                    let tone = 0.82 + 0.3 * hash2(col as i32, row as i32, 6);
                    Surface {
                        albedo: scale(base, tone * (0.92 + 0.12 * grain)),
                        paint_hold: 0.88,
                        relief: 1.0,
                        sheen: 0.0,
                    }
                }
            }
            Material::Plaster => {
                let g = value_noise(p, 0.02, 7);
                Surface {
                    albedo: scale(base, 0.96 + 0.06 * g),
                    paint_hold: 1.0,
                    relief: 1.0,
                    sheen: 0.0,
                }
            }
        }
    }
}

/// Illumination (linear RGB irradiance factor) at wall point `p` and the key
/// light's share of it (for glare).
fn illumination(lighting: Lighting, p: Vec2, view: &View, work: Option<Bounds>) -> ([f32; 3], f32) {
    // 0 at the bottom of the view, 1 at the top; 0 left, 1 right.
    let ny = ((p.y - (view.top_v - view.height_m())) / view.height_m()).clamp(0.0, 1.0);
    let nx = ((p.x - view.left_u) / view.width_m()).clamp(0.0, 1.0);
    let (ambient, key, key_amount) = match lighting {
        Lighting::Day => ([0.30, 0.34, 0.40], [1.00, 0.96, 0.88], 0.88 + 0.12 * ny),
        Lighting::Overcast => ([0.78, 0.80, 0.84], [0.0; 3], 0.0),
        Lighting::Dusk => ([0.16, 0.13, 0.22], [1.00, 0.52, 0.26], 0.75 - 0.45 * nx),
        Lighting::Night => {
            // Point light out from the wall above the work: E ∝ cos³θ / h².
            let anchor = work.map_or(Vec2::new(0.0, 0.5), |b| Vec2::new(b.center().x, b.max.y));
            let light = Vec2::new(anchor.x, anchor.y + STREETLIGHT_ABOVE_M);
            let d = p - light;
            let r2 = d.x * d.x + d.y * d.y;
            let h2 = STREETLIGHT_OUT_M * STREETLIGHT_OUT_M;
            let falloff = (h2 / (h2 + r2)).powf(1.5);
            ([0.018, 0.022, 0.04], [1.00, 0.82, 0.55], 2.6 * falloff)
        }
    };
    let e = [
        ambient[0] + key[0] * key_amount,
        ambient[1] + key[1] * key_amount,
        ambient[2] + key[2] * key_amount,
    ];
    (e, key_amount * (key[0] + key[1] + key[2]) / 3.0)
}

fn tone(c: f32) -> u8 {
    let mapped = 1.0 - (-c.max(0.0) * EXPOSURE).exp();
    (linear_to_srgb(mapped) * 255.0 + 0.5) as u8
}

/// Composite `layer` onto the scene's wall and light it. Returns opaque
/// RGBA8, row-major from the top-left.
pub fn compose(layer: &PaintLayer, scene: Scene, work: Option<Bounds>) -> Vec<u8> {
    let view = layer.view;
    let base = lin(WALL_COLORS
        .get(scene.wall_color as usize)
        .and_then(|(_, c)| *c)
        .unwrap_or(scene.material.natural()));
    let mut out = Vec::with_capacity(view.width as usize * view.height as usize * 4);
    for y in 0..view.height {
        for x in 0..view.width {
            let p = view.wall_at(x, y);
            let s = scene.material.surface(p, base);
            let paint = layer.get(x, y);
            let held = s.paint_hold;
            let cover = paint[3] * held;
            let (e, key) = illumination(scene.lighting, p, &view, work);
            let glare = s.sheen * key * (1.0 - cover);
            for c in 0..3 {
                let surface = s.albedo[c] * (1.0 - cover) + paint[c] * held;
                out.push(tone(surface * e[c] * s.relief + glare));
            }
            out.push(255);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_bounds() -> Bounds {
        Bounds {
            min: Vec2::new(-0.5, 0.0),
            max: Vec2::new(0.5, 0.6),
        }
    }

    fn luminance(rgba: &[u8]) -> f32 {
        rgba.chunks(4)
            .map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32)
            .sum::<f32>()
            / (rgba.len() / 4) as f32
    }

    fn blank(view: View, scene: Scene) -> Vec<u8> {
        compose(&PaintLayer::new(view), scene, Some(work_bounds()))
    }

    fn scene(lighting: Lighting, material: Material) -> Scene {
        Scene {
            lighting,
            material,
            wall_color: 0,
        }
    }

    #[test]
    fn distance_framing_follows_the_camera_fov() {
        let near = view_for(Some(work_bounds()), Framing::Distance(2.0), 400, 300);
        let far = view_for(Some(work_bounds()), Framing::Distance(8.0), 400, 300);
        let expected = 2.0 * 2.0 * (30f32).to_radians().tan();
        assert!((near.width_m() - expected).abs() < 1e-4);
        // 4x the distance -> the work spans 1/4 the pixels.
        assert!((far.m_per_px / near.m_per_px - 4.0).abs() < 1e-4);
        // Centered on the work.
        let c = near.wall_at(200, 150);
        assert!(c.x.abs() < near.m_per_px && (c.y - 0.3).abs() < near.m_per_px);
    }

    #[test]
    fn distance_is_clamped() {
        assert_eq!(clamp_distance(0.0), MIN_VIEW_DISTANCE_M);
        assert_eq!(clamp_distance(1e6), MAX_VIEW_DISTANCE_M);
        assert_eq!(clamp_distance(f32::NAN), DEFAULT_VIEW_DISTANCE_M);
    }

    #[test]
    fn fit_framing_contains_the_work_with_margin() {
        let v = view_for(Some(work_bounds()), Framing::FitWork, 300, 300);
        assert!(v.width_m() >= 1.0 * (1.0 + 2.0 * FIT_MARGIN) - 1e-4);
        assert!(v.left_u < -0.5 && v.top_v > 0.6);
        // Empty works still get a sane view.
        let e = view_for(None, Framing::FitWork, 100, 100);
        assert!(e.m_per_px > 0.0 && e.width_m() >= FIT_MIN_SIZE_M);
    }

    #[test]
    fn output_is_opaque_and_deterministic() {
        let view = view_for(Some(work_bounds()), Framing::Distance(3.0), 64, 48);
        let a = blank(view, scene(Lighting::Day, Material::Brick));
        assert_eq!(a.len(), 64 * 48 * 4);
        assert!(a.chunks(4).all(|p| p[3] == 255));
        assert_eq!(a, blank(view, scene(Lighting::Day, Material::Brick)));
    }

    #[test]
    fn night_is_darker_than_day_and_lit_under_the_street_light() {
        let view = view_for(Some(work_bounds()), Framing::Distance(6.0), 120, 90);
        let day = blank(view, scene(Lighting::Day, Material::Concrete));
        let night = blank(view, scene(Lighting::Night, Material::Concrete));
        assert!(luminance(&night) < luminance(&day) * 0.7);
        // Pool of light above the work vs. the dark bottom corner.
        let px = |img: &[u8], x: u32, y: u32| img[((y * 120 + x) * 4) as usize];
        let light_x = ((0.0 - view.left_u) / view.m_per_px) as u32;
        let light_y = ((view.top_v - (0.6 + STREETLIGHT_ABOVE_M)) / view.m_per_px) as u32;
        assert!(px(&night, light_x, light_y) > px(&night, 2, 88).saturating_add(60));
    }

    #[test]
    fn dusk_rakes_from_the_left() {
        let view = view_for(None, Framing::Distance(3.0), 100, 20);
        let img = blank(view, scene(Lighting::Dusk, Material::Plaster));
        let left: f32 = luminance(&img[..(20 * 4)]);
        let right: f32 = luminance(&img[(80 * 4)..(100 * 4)]);
        assert!(left > right);
    }

    #[test]
    fn wall_color_recolors_the_wall_not_the_paint() {
        let view = View::centered(Vec2::new(0.0, 0.0), 0.01, 40, 40);
        let mut layer = PaintLayer::new(view);
        let red = ColorF {
            r: 230,
            g: 20,
            b: 20,
            a: 255,
        };
        layer.stamp_dab(
            &Dab::circle(Vec2::new(0.0, 0.0), 0.12, 1.0, 0.0),
            red.into(),
        );
        let s = |wall_color| Scene {
            lighting: Lighting::Overcast,
            material: Material::Plaster,
            wall_color,
        };
        let white = compose(&layer, s(1), None);
        let navy = compose(&layer, s(5), None);
        let at = |img: &[u8], x: u32, y: u32| {
            let i = ((y * 40 + x) * 4) as usize;
            [img[i] as i32, img[i + 1] as i32, img[i + 2] as i32]
        };
        // Bare corner: very different.
        let (w, n) = (at(&white, 1, 1), at(&navy, 1, 1));
        assert!((w[0] - n[0]).abs() > 100);
        // Opaque paint core: almost unchanged, and red.
        let (w, n) = (at(&white, 20, 20), at(&navy, 20, 20));
        assert!((w[0] - n[0]).abs() < 12 && w[0] > 150 && w[1] < 80);
    }

    #[test]
    fn shutter_has_horizontal_ribs_and_concrete_grain() {
        let view = View::centered(Vec2::new(0.0, 0.0), 0.004, 4, 100);
        let img = blank(view, scene(Lighting::Overcast, Material::Shutter));
        let rows: Vec<i32> = (0..100).map(|y| img[(y * 4 * 4) as usize] as i32).collect();
        let span = rows.iter().max().unwrap() - rows.iter().min().unwrap();
        assert!(span > 20, "rib contrast {span}");
        // Along a rib (same row), the shutter is uniform.
        let row = &img[..16];
        assert!((row[0] as i32 - row[8] as i32).abs() < 8);

        let view = View::centered(Vec2::new(0.0, 0.0), 0.002, 64, 1);
        let img = blank(view, scene(Lighting::Overcast, Material::Concrete));
        let vals: Vec<i32> = img.chunks(4).map(|p| p[0] as i32).collect();
        assert!(vals.iter().max().unwrap() - vals.iter().min().unwrap() > 8);
    }

    #[test]
    fn mortar_holds_less_paint_than_brick() {
        let s = Material::Brick;
        let base = lin(s.natural());
        let mortar = s.surface(Vec2::new(0.1, 0.004), base);
        let brick = s.surface(Vec2::new(0.1, 0.04), base);
        assert!(mortar.paint_hold < brick.paint_hold);
        assert!(mortar.relief < brick.relief);
    }

    #[test]
    fn bounds_cover_dabs_and_drips() {
        let dabs = [Dab::circle(Vec2::new(0.2, 0.3), 0.05, 1.0, 0.5)];
        let drips = [Drip {
            uv: Vec2::new(-0.1, 0.0),
            len: 0.3,
            width: 0.01,
            opacity: 1.0,
        }];
        let b = Bounds::of(dabs.iter(), drips.iter()).unwrap();
        assert!((b.min.x + 0.105).abs() < 1e-6 && (b.min.y + 0.3).abs() < 1e-6);
        assert!((b.max.x - 0.25).abs() < 1e-6 && (b.max.y - 0.35).abs() < 1e-6);
        assert!(Bounds::of([].iter(), [].iter()).is_none());
    }
}
