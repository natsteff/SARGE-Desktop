use crate::{
    engine,
    model::{self, Photo, Result, Source},
    volumes,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};
use rusqlite::{params, Connection};
use std::{
    fs,
    io::{BufReader, Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn enabled(c: &Connection) -> bool {
    c.query_row("SELECT value FROM settings WHERE key='previews'", [], |r| {
        r.get::<_, String>(0)
    })
    .ok()
    .is_some_and(|v| v == "true")
}
pub fn original(source: &Source, file: &Photo) -> Result<PathBuf> {
    if !file.present || !volumes::available(&source.path, &source.volume_id) {
        return Err(
            "The original photo is offline or missing. Reconnect the correct drive first.".into(),
        );
    }
    let rel = Path::new(&file.path);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("Invalid catalog path".into());
    }
    let base = volumes::canonical(&source.path)?;
    let actual = fs::canonicalize(base.join(rel)).map_err(|e| e.to_string())?;
    if !actual.starts_with(base) {
        return Err("Original photo escapes the source through a link.".into());
    }
    let ext = actual
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if !["cr2", "jpg", "jpeg", "png"].contains(&ext.as_str()) {
        return Err("Unsupported photo type".into());
    }
    Ok(actual)
}
fn decode<R: std::io::BufRead + std::io::Seek>(reader: R) -> Option<DynamicImage> {
    let mut reader = ImageReader::new(reader).with_guessed_format().ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(20000);
    limits.max_image_height = Some(20000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        image.apply_orientation(o);
    }
    Some(image)
}
pub fn image(path: &Path) -> Option<DynamicImage> {
    if path
        .extension()?
        .to_string_lossy()
        .eq_ignore_ascii_case("cr2")
    {
        // Extract an embedded camera JPEG, never demosaic or alter RAW data.
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(32 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .ok()?;
        let mut attempts = 0;
        for (start, _) in bytes
            .windows(3)
            .enumerate()
            .filter(|(_, w)| *w == [0xff, 0xd8, 0xff])
        {
            if attempts >= 8 {
                break;
            }
            attempts += 1;
            let end = (start + 3 + 16 * 1024 * 1024).min(bytes.len());
            if let Some(i) = bytes[start + 3..end]
                .windows(2)
                .position(|w| w == [0xff, 0xd9])
            {
                if let Some(img) = decode(Cursor::new(&bytes[start..start + 3 + i + 2])) {
                    return Some(img);
                }
            }
        }
        None
    } else {
        decode(BufReader::new(fs::File::open(path).ok()?))
    }
}
pub fn generate(
    c: &Connection,
    source: &Source,
    job: &str,
    cancel: &AtomicBool,
) -> Result<(i64, i64)> {
    if !enabled(c) {
        return Ok((0, 0));
    }
    let files = model::photos(c)?;
    let sources = model::sources(c)?;
    let file_index: std::collections::HashMap<_, _> = files.iter().map(|f| (f.id, f)).collect();
    let source_index: std::collections::HashMap<_, _> =
        sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut count = 0;
    let mut unavailable = 0;
    for file in files
        .iter()
        .filter(|f| f.source_id == source.id && f.present)
    {
        if cancel.load(Ordering::Relaxed) {
            return Err("Preview generation cancelled; existing previews were retained.".into());
        }
        let missing_only: bool = c
            .query_row("SELECT mode='previews' FROM jobs WHERE id=?", [job], |r| {
                r.get(0)
            })
            .unwrap_or(false);
        if missing_only && file.preview {
            count += 1;
            continue;
        }
        let mut candidates = vec![(source, file, "Original image".to_string())];
        if file.kind == "raw" {
            let mut q=c.prepare("SELECT edited_id FROM matches WHERE raw_id=? AND status='confirmed' ORDER BY edited_id").map_err(|e|e.to_string())?;
            let ids = q
                .query_map([file.id], |r| r.get::<_, i64>(0))
                .map_err(|e| e.to_string())?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for id in ids {
                if let Some(edit) = file_index.get(&id).filter(|e| e.present) {
                    if let Some(s) = source_index
                        .get(edit.source_id.as_str())
                        .filter(|s| !s.archived)
                    {
                        candidates.push((s, edit, format!("Matched export: {}", edit.name)));
                    }
                }
            }
        }
        let mut stored = false;
        for (candidate_source, candidate, origin) in candidates {
            let stamp = format!("{}:{}:{}", candidate.id, candidate.size, candidate.mtime);
            let cached:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM previews WHERE file_id=? AND mtime=? AND size=? AND stamp=?)",params![file.id,file.mtime,file.size,stamp],|r|r.get(0)).map_err(|e|e.to_string())?;
            if cached {
                stored = true;
                break;
            }
            let path = match original(candidate_source, candidate) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let before = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if before.len() as i64 != candidate.size || engine::mtime(&before)? != candidate.mtime {
                continue;
            }
            if let Some(img) = image(&path) {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Preview generation cancelled".into());
                }
                let after = fs::metadata(&path).map_err(|e| e.to_string())?;
                if before.len() != after.len() || engine::mtime(&before)? != engine::mtime(&after)?
                {
                    continue;
                }
                let thumbnail = img.thumbnail(160, 160).to_rgb8();
                let mut data = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 75)
                    .encode_image(&thumbnail)
                    .map_err(|e| e.to_string())?;
                c.execute("INSERT INTO previews(file_id,mtime,size,stamp,data,origin) VALUES(?,?,?,?,?,?) ON CONFLICT(file_id) DO UPDATE SET mtime=excluded.mtime,size=excluded.size,stamp=excluded.stamp,data=excluded.data,origin=excluded.origin",params![file.id,file.mtime,file.size,stamp,data,if file.kind=="raw"&&candidate.id==file.id{"Embedded RAW JPEG".to_string()}else{origin}]).map_err(|e|e.to_string())?;
                stored = true;
                break;
            }
        }
        if stored {
            count += 1
        } else {
            unavailable += 1;
            c.execute("DELETE FROM previews WHERE file_id=?", [file.id])
                .map_err(|e| e.to_string())?;
        }
        c.execute(
            "UPDATE jobs SET previews=?,unavailable=? WHERE id=?",
            params![count, unavailable, job],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok((count, unavailable))
}
pub fn fetch(c: &Connection, ids: &[i64]) -> Result<serde_json::Value> {
    if ids.len() > 100 {
        return Err("Request at most 100 previews.".into());
    }
    let mut out = serde_json::Map::new();
    if !enabled(c) {
        return Ok(out.into());
    }
    for id in ids {
        let row=c.query_row("SELECT p.data,p.origin FROM previews p JOIN files f ON f.id=p.file_id JOIN sources s ON s.id=f.source_id WHERE p.file_id=? AND p.mtime=f.mtime AND p.size=f.size AND f.present AND NOT s.archived",[id],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?))).ok();
        if let Some((data, origin)) = row {
            out.insert(
                id.to_string(),
                serde_json::json!({"data":STANDARD.encode(data),"origin":origin}),
            );
        }
    }
    Ok(out.into())
}

pub fn invalidate_related(c: &Connection) -> Result<()> {
    c.execute("DELETE FROM previews WHERE origin LIKE 'Matched export:%' AND NOT EXISTS(SELECT 1 FROM matches m JOIN files e ON e.id=m.edited_id JOIN sources s ON s.id=e.source_id WHERE m.raw_id=previews.file_id AND m.status='confirmed' AND e.present AND NOT s.archived AND previews.stamp=CAST(e.id AS TEXT)||':'||CAST(e.size AS TEXT)||':'||e.mtime)",[]).map_err(|e|e.to_string())?;
    Ok(())
}
