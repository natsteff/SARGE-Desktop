// Best-effort, bounded native container metadata. No video decoding or thumbnails.
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
fn be32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}
fn be64(b: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(..8)?.try_into().ok()?))
}
fn le32(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?))
}
fn bytes(f: &mut File, pos: u64, n: usize) -> Option<Vec<u8>> {
    let mut b = vec![0; n];
    f.seek(SeekFrom::Start(pos)).ok()?;
    f.read_exact(&mut b).ok()?;
    Some(b)
}
#[derive(Default)]
struct Info {
    duration: Option<f64>,
    width: u32,
    height: u32,
    rotation: i32,
}
impl Info {
    fn value(self) -> Value {
        let mut out = json!({"VideoMetadataVersion":1});
        if let Some(d) = self.duration.filter(|d| d.is_finite() && *d >= 0.0) {
            out["DurationSeconds"] = json!(d)
        }
        if self.width > 0 && self.height > 0 && self.width <= 100000 && self.height <= 100000 {
            let (w, h) = if self.rotation.abs() == 90 {
                (self.height, self.width)
            } else {
                (self.width, self.height)
            };
            out["ImageWidth"] = json!(w);
            out["ImageLength"] = json!(h);
        }
        out
    }
}
fn movie(f: &mut File, start: u64, end: u64, depth: u8, budget: &mut usize, info: &mut Info) {
    if depth > 8 {
        return;
    }
    let mut p = start;
    while p + 8 <= end && *budget > 0 {
        *budget -= 1;
        let Some(h) = bytes(f, p, 8) else { return };
        let typ = &h[4..8];
        let mut size = be32(&h).unwrap() as u64;
        let mut head = 8;
        if size == 1 {
            let Some(v) = bytes(f, p + 8, 8).and_then(|b| be64(&b)) else {
                return;
            };
            size = v;
            head = 16
        } else if size == 0 {
            size = end - p
        }
        if size < head || size > end - p {
            return;
        }
        let data = p + head;
        if [b"moov".as_slice(), b"trak", b"mdia"].contains(&typ) {
            movie(f, data, p + size, depth + 1, budget, info)
        } else if typ == b"mvhd" {
            if let Some(b) = bytes(f, data, (size - head).min(40) as usize) {
                let pair = if b.first() == Some(&1) {
                    be32(b.get(20..).unwrap_or(&[])).zip(be64(b.get(24..).unwrap_or(&[])))
                } else {
                    be32(b.get(12..).unwrap_or(&[]))
                        .zip(be32(b.get(16..).unwrap_or(&[])).map(u64::from))
                };
                if let Some((scale, duration)) = pair {
                    if scale > 0 && duration != u64::MAX && duration != u32::MAX as u64 {
                        info.duration = Some(duration as f64 / scale as f64)
                    }
                }
            }
        } else if typ == b"tkhd" {
            if let Some(b) = bytes(f, data, (size - head).min(104) as usize) {
                let offset = if b.first() == Some(&1) { 88 } else { 76 };
                if let (Some(w), Some(h)) = (
                    b.get(offset..).and_then(be32),
                    b.get(offset + 4..).and_then(be32),
                ) {
                    let (w, h) = (w >> 16, h >> 16);
                    if w.saturating_mul(h) > info.width.saturating_mul(info.height) {
                        info.width = w;
                        info.height = h;
                        let matrix = if offset == 88 { 52 } else { 40 };
                        let a = b.get(matrix..).and_then(be32).unwrap_or(0) as i32;
                        let v = b.get(matrix + 4..).and_then(be32).unwrap_or(0) as i32;
                        info.rotation = if a == 0 && v.unsigned_abs() == 65536 {
                            90
                        } else {
                            0
                        };
                    }
                }
            }
        }
        p += size;
    }
}
fn vint(b: &[u8], id: bool) -> Option<(u64, usize)> {
    let first = *b.first()?;
    let n = first.leading_zeros() as usize + 1;
    if n > 8 || b.len() < n {
        return None;
    }
    let mut value = if id {
        first as u64
    } else {
        (first & ((1u16 << (8 - n)) - 1) as u8) as u64
    };
    for byte in &b[1..n] {
        value = (value << 8) | *byte as u64
    }
    Some((value, n))
}
fn uint(b: &[u8]) -> u64 {
    b.iter().take(8).fold(0, |n, v| (n << 8) | *v as u64)
}
fn ebml(
    b: &[u8],
    depth: u8,
    budget: &mut usize,
    scale: &mut u64,
    duration: &mut Option<f64>,
    info: &mut Info,
) {
    if depth > 10 {
        return;
    }
    let mut p = 0;
    while p < b.len() && *budget > 0 {
        *budget -= 1;
        let Some((id, n)) = vint(&b[p..], true) else {
            return;
        };
        let Some((size, m)) = vint(&b[p + n..], false) else {
            return;
        };
        let start = p + n + m;
        let available = b.len() - start;
        let len = (size.min(available as u64)) as usize;
        let value = &b[start..start + len];
        match id {
            0x18538067 | 0x1549a966 | 0x1654ae6b | 0xae | 0xe0 => {
                ebml(value, depth + 1, budget, scale, duration, info)
            }
            0x2ad7b1 => *scale = uint(value),
            0x4489 => {
                *duration = match value.len() {
                    4 => Some(f32::from_be_bytes(value.try_into().unwrap()) as f64),
                    8 => Some(f64::from_be_bytes(value.try_into().unwrap())),
                    _ => None,
                }
            }
            0xb0 => info.width = uint(value).min(u32::MAX as u64) as u32,
            0xba => info.height = uint(value).min(u32::MAX as u64) as u32,
            _ => {}
        }
        if size > available as u64 {
            return;
        }
        p = start + len;
    }
}
fn avi(b: &[u8], info: &mut Info) {
    for (i, v) in b.windows(4).enumerate() {
        if v == b"avih" {
            if let Some(d) = b.get(i + 8..i + 48) {
                if let (Some(us), Some(frames), Some(w), Some(h)) =
                    (le32(d), le32(&d[16..]), le32(&d[32..]), le32(&d[36..]))
                {
                    info.duration = Some(us as f64 * frames as f64 / 1e6);
                    info.width = w;
                    info.height = h;
                    return;
                }
            }
        }
    }
}
pub fn metadata(path: &Path) -> Value {
    let mut info = Info::default();
    let Ok(mut f) = File::open(path) else {
        return info.value();
    };
    let Ok(meta) = f.metadata() else {
        return info.value();
    };
    let ext = crate::formats::extension(&path.to_string_lossy());
    if ["mp4", "mov", "m4v", "3gp", "3g2", "qt"].contains(&ext.as_str()) {
        movie(&mut f, 0, meta.len(), 0, &mut 20000, &mut info)
    } else if ["mkv", "webm", "avi", "divx"].contains(&ext.as_str()) {
        if let Some(b) = bytes(&mut f, 0, meta.len().min(16 * 1024 * 1024) as usize) {
            if ["mkv", "webm"].contains(&ext.as_str()) {
                let mut scale = 1_000_000;
                let mut duration = None;
                ebml(&b, 0, &mut 50000, &mut scale, &mut duration, &mut info);
                info.duration = duration.map(|d| d * scale as f64 / 1e9)
            } else if ["avi", "divx"].contains(&ext.as_str())
                && b.starts_with(b"RIFF")
                && b.get(8..12) == Some(b"AVI ")
            {
                avi(&b, &mut info)
            }
        }
    }
    info.value()
}
