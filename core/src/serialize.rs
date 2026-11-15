//! Canvas wire format (little-endian), designed to be uploaded and
//! re-anchored in later phases.
//!
//! ```text
//! offset  field
//! 0       magic "GFXC"                  u32
//! 4       format version = 2            u16  (v1 payloads still decode)
//! 6       header_len                    u16  (bytes before stroke data)
//! 8       created_unix_time             u64
//! 16      has_frame                     u8
//! 17      (reserved)                    u8 x 3
//! 20      frame.origin                  f32 x 3   (only if has_frame)
//! 32      frame.axis_u                  f32 x 3
//! 44      frame.axis_v                  f32 x 3
//! 56      (normal = u x v, not stored)
//! 56/20   stroke_count                  u32       ("body" starts here)
//! +4      per stroke: palette_index     u8
//!         reserved                      u8 x 3
//!         dab_count                     u32
//!         per dab (v2):                 f32 x 9
//!           u, v, radius_major, radius_minor, rotation,
//!           gradient_u, gradient_v, opacity, softness
//!         drip_count                    u32  (v2 only)
//!         per drip: u, v, len, width, opacity  f32 x 5
//! end-4   crc32 (IEEE) of all preceding bytes   u32
//! ```
//!
//! v1 note: dabs were 4 floats (u, v, radius, opacity) and there were no
//! drips. v1 data decodes into v2's model as circular dabs (major = minor =
//! radius, no gradient, default softness 0.5).
//!
//! Strokes are pure wall-local geometry relative to `frame`; nothing in the
//! format ties a work to a world position, so a later phase can attach a
//! location/wall anchor when the work is uploaded.

use crate::canvas::{Canvas, Dab, Drip, Stroke};
use crate::math::{PlaneFrame, Vec2, Vec3};

const MAGIC: u32 = 0x4746_5843; // "GFXC"
const VERSION: u16 = 2;
const HEADER_LEN: u16 = 20;

/// Minimum sane length of a dab section before we alloc.
const MAX_STROKES: usize = 4096;
const MAX_DABS_PER_STROKE: usize = 1 << 20;
const MAX_DRIPS_PER_STROKE: usize = 1 << 20;
/// Default edge softness for v1 dabs (which carried no softness field).
const V1_SOFTNESS: f32 = 0.5;

pub fn encode(canvas: &Canvas, created_unix_time: u64) -> Vec<u8> {
    let has_frame = canvas.frame().is_some();
    let mut out = Vec::with_capacity(HEADER_LEN as usize + 64);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&HEADER_LEN.to_le_bytes());
    out.extend_from_slice(&created_unix_time.to_le_bytes());
    out.push(has_frame as u8);
    out.extend_from_slice(&[0; 3]);
    debug_assert_eq!(out.len(), HEADER_LEN as usize);

    if let Some(f) = canvas.frame() {
        push_v3(&mut out, f.origin);
        push_v3(&mut out, f.u);
        push_v3(&mut out, f.v);
    }

    push_u32(&mut out, canvas.strokes().len() as u32);
    for s in canvas.strokes() {
        out.push(s.palette_index);
        out.extend_from_slice(&[0; 3]);
        push_u32(&mut out, s.dabs.len() as u32);
        for d in &s.dabs {
            push_f32(&mut out, d.uv.x);
            push_f32(&mut out, d.uv.y);
            push_f32(&mut out, d.radius_major);
            push_f32(&mut out, d.radius_minor);
            push_f32(&mut out, d.rotation);
            push_f32(&mut out, d.gradient.x);
            push_f32(&mut out, d.gradient.y);
            push_f32(&mut out, d.opacity);
            push_f32(&mut out, d.softness);
        }
        push_u32(&mut out, s.drips.len() as u32);
        for d in &s.drips {
            push_f32(&mut out, d.uv.x);
            push_f32(&mut out, d.uv.y);
            push_f32(&mut out, d.len);
            push_f32(&mut out, d.width);
            push_f32(&mut out, d.opacity);
        }
    }

    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

pub fn decode(bytes: &[u8]) -> Result<(Option<PlaneFrame>, Vec<Stroke>, u64), String> {
    if bytes.len() < HEADER_LEN as usize + 4 + 4 {
        return Err("truncated header".into());
    }
    let (body, crc_bytes) = bytes.split_at(bytes.len() - 4);
    let stored_crc = u32::from_le_bytes(crc_bytes.try_into().unwrap());
    if crc32(body) != stored_crc {
        return Err("crc mismatch".into());
    }
    let mut r = Reader::new(body);
    if r.u32()? != MAGIC {
        return Err("bad magic".into());
    }
    let version = r.u16()?;
    if !(1..=VERSION).contains(&version) {
        return Err(format!("unsupported version {version}"));
    }
    let header_len = r.u16()? as usize;
    let created = r.u64()?;
    let has_frame = r.u8()? != 0;
    let _reserved = r.bytes(3)?;
    if header_len < HEADER_LEN as usize {
        return Err("bad header_len".into());
    }
    // Skip forward-compatible header extension bytes.
    r.skip(header_len - HEADER_LEN as usize)?;

    let frame = if has_frame {
        let origin = r.v3()?;
        let u = r.v3()?;
        let v = r.v3()?;
        let normal = u.cross(v).normalized();
        if normal.len() < 0.5 {
            return Err("degenerate canvas frame".into());
        }
        Some(PlaneFrame {
            origin,
            u: u.normalized(),
            v,
            normal,
        })
    } else {
        None
    };

    let stroke_count = r.u32()? as usize;
    if stroke_count > MAX_STROKES {
        return Err("stroke_count too large".into());
    }
    let mut strokes = Vec::with_capacity(stroke_count.min(1024));
    for _ in 0..stroke_count {
        let palette_index = r.u8()?;
        let _reserved = r.bytes(3)?;
        let dab_count = r.u32()? as usize;
        if dab_count > MAX_DABS_PER_STROKE {
            return Err("dab_count too large".into());
        }
        // Bounds-check the whole dab block before allocating.
        let dab_bytes: usize = if version >= 2 { 36 } else { 16 };
        r.require(dab_count.checked_mul(dab_bytes).ok_or("dab block overflow")?)?;
        let mut dabs = Vec::with_capacity(dab_count.min(65536));
        for _ in 0..dab_count {
            let uv = Vec2::new(r.f32()?, r.f32()?);
            let dab = if version >= 2 {
                Dab {
                    uv,
                    radius_major: r.f32()?.clamp(0.0, 10.0),
                    radius_minor: r.f32()?.clamp(0.0, 10.0),
                    rotation: r.f32()?,
                    gradient: Vec2::new(r.f32()?, r.f32()?),
                    opacity: r.f32()?.clamp(0.0, 1.0),
                    softness: r.f32()?.clamp(0.0, 1.0),
                }
            } else {
                // v1: a soft disc — maps to a circle in the v2 model.
                let radius = r.f32()?.clamp(0.0, 10.0);
                let opacity = r.f32()?.clamp(0.0, 1.0);
                Dab::circle(uv, radius, opacity, V1_SOFTNESS)
            };
            if !uv.x.is_finite()
                || !uv.y.is_finite()
                || !dab.radius_major.is_finite()
                || !dab.radius_minor.is_finite()
                || !dab.rotation.is_finite()
                || !dab.gradient.x.is_finite()
                || !dab.gradient.y.is_finite()
            {
                return Err("non-finite dab".into());
            }
            dabs.push(dab);
        }
        // Drips exist only in v2+; v1 strokes never saturated (model had no
        // oversaturation tracking at all).
        let mut drips = Vec::new();
        if version >= 2 {
            let drip_count = r.u32()? as usize;
            if drip_count > MAX_DRIPS_PER_STROKE {
                return Err("drip_count too large".into());
            }
            r.require(drip_count.checked_mul(20).ok_or("drip block overflow")?)?;
            drips.reserve(drip_count.min(65536));
            for _ in 0..drip_count {
                let uv = Vec2::new(r.f32()?, r.f32()?);
                let len = r.f32()?;
                let width = r.f32()?;
                let opacity = r.f32()?;
                if !uv.x.is_finite()
                    || !uv.y.is_finite()
                    || !len.is_finite()
                    || !width.is_finite()
                {
                    return Err("non-finite drip".into());
                }
                drips.push(Drip {
                    uv,
                    len: len.clamp(0.0, 10.0),
                    width: width.clamp(0.0, 1.0),
                    opacity: opacity.clamp(0.0, 1.0),
                });
            }
        }
        strokes.push(Stroke {
            palette_index,
            dabs,
            drips,
        });
    }
    Ok((frame, strokes, created))
}

fn push_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn push_v3(out: &mut Vec<u8>, v: Vec3) {
    push_f32(out, v.x);
    push_f32(out, v.y);
    push_f32(out, v.z);
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn require(&self, n: usize) -> Result<(), String> {
        if self.pos + n > self.bytes.len() {
            Err("unexpected end of data".into())
        } else {
            Ok(())
        }
    }
    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.require(n)?;
        self.pos += n;
        Ok(())
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        self.require(n)?;
        let s = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn v3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
}

/// CRC-32 (IEEE 802.3) — small table generated at compile time so we don't
/// need a dependency.
const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = crc_table();
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc = TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Plane, Vec3};

    fn sample_canvas() -> Canvas {
        let mut c = Canvas::new();
        c.set_frame(PlaneFrame::facing(
            Plane {
                point: Vec3::new(0.0, 1.5, 2.0),
                normal: Vec3::new(0.0, 0.0, -1.0),
            },
            Vec3::new(0.0, 1.5, 0.0),
            Vec3::UP,
        ));
        c.commit_stroke(Stroke {
            palette_index: 3,
            dabs: vec![
                Dab {
                    uv: Vec2::new(0.1, 0.2),
                    radius_major: 0.08,
                    radius_minor: 0.05,
                    rotation: 0.35,
                    gradient: Vec2::new(-0.94, -0.34),
                    opacity: 0.8,
                    softness: 0.4,
                },
                Dab::circle(Vec2::new(-0.3, 1.1), 0.12, 0.3, 0.5),
            ],
            drips: vec![Drip {
                uv: Vec2::new(0.11, 0.2),
                len: 0.2,
                width: 0.006,
                opacity: 0.9,
            }],
        });
        c.commit_stroke(Stroke {
            palette_index: 8,
            dabs: vec![Dab::circle(Vec2::new(0.0, 0.0), 0.01, 1.0, 0.6)],
            drips: vec![],
        });
        c
    }

    #[test]
    fn round_trip_byte_exact() {
        let c = sample_canvas();
        let bytes = encode(&c, 1_700_000_000);
        let (frame, strokes, created) = decode(&bytes).unwrap();
        assert_eq!(created, 1_700_000_000);
        assert_eq!(strokes, *c.strokes());
        let f = frame.unwrap();
        let orig = c.frame().unwrap();
        assert!((f.origin - orig.origin).len() < 1e-6);
        assert!((f.u - orig.u).len() < 1e-6);
        assert!((f.v - orig.v).len() < 1e-6);
        // Re-encoding the decoded canvas yields identical bytes.
        let mut c2 = Canvas::new();
        c2.replace(frame, strokes);
        assert_eq!(encode(&c2, 1_700_000_000), bytes);
    }

    #[test]
    fn decode_rejects_corruption() {
        let c = sample_canvas();
        let bytes = encode(&c, 0);
        // Truncated.
        assert!(decode(&bytes[..bytes.len() - 8]).is_err());
        // Bit-flip breaks the CRC.
        let mut bad = bytes.clone();
        bad[30] ^= 0xFF;
        assert!(decode(&bad).is_err());
        // Bad magic.
        let mut bad = bytes;
        bad[0] = b'X';
        assert!(decode(&bad).is_err());
    }

    /// Hand-rolled v1 bytes (4-float dabs, no drip section) matching the old
    /// wire format — proves saved v1 works still load.
    fn encode_v1(dabs: &[(f32, f32, f32, f32)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // version = 1
        out.extend_from_slice(&HEADER_LEN.to_le_bytes());
        out.extend_from_slice(&1_700_000_000u64.to_le_bytes());
        out.push(0u8); // has_frame = false
        out.extend_from_slice(&[0; 3]);
        out.extend_from_slice(&1u32.to_le_bytes()); // stroke_count
        out.push(2u8); // palette_index
        out.extend_from_slice(&[0; 3]);
        out.extend_from_slice(&(dabs.len() as u32).to_le_bytes());
        for &(u, v, r, o) in dabs {
            for f in [u, v, r, o] {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        let crc = crc32(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }

    #[test]
    fn v1_payloads_still_decode() {
        let bytes = encode_v1(&[(0.1, 0.2, 0.05, 0.8), (-0.3, 1.1, 0.12, 0.3)]);
        let (frame, strokes, created) = decode(&bytes).unwrap();
        assert!(frame.is_none());
        assert_eq!(created, 1_700_000_000);
        assert_eq!(strokes.len(), 1);
        let s = &strokes[0];
        assert_eq!(s.palette_index, 2);
        assert_eq!(s.dabs.len(), 2);
        // v1 disc -> v2 circle: major == minor == old radius.
        let d = &s.dabs[0];
        assert!((d.radius_major - 0.05).abs() < 1e-7);
        assert_eq!(d.radius_major, d.radius_minor);
        assert_eq!(d.rotation, 0.0);
        assert_eq!(d.gradient, Vec2::new(0.0, 0.0));
        assert_eq!(d.softness, V1_SOFTNESS);
        assert!(s.drips.is_empty());
    }

    #[test]
    fn crc32_known_vector() {
        // IEEE CRC-32 of "123456789" is 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
