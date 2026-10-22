use crate::model::{self, Photo, Result, Source};
use regex::Regex;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    time::UNIX_EPOCH,
};

pub fn folder_details(name: &str) -> (String, String) {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"^(\d{4})_?(\d{2})_?(\d{2})([^\d].*|$)").unwrap());
    if let Some(c) = re.captures(name) {
        let d = format!("{}-{}-{}", &c[1], &c[2], &c[3]);
        if chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_ok() {
            let desc = c[4].trim_matches([' ', '_', '-']);
            return (
                d,
                if desc.is_empty() {
                    name.into()
                } else {
                    desc.into()
                },
            );
        }
    }
    (String::new(), name.into())
}
pub fn attributes(path: &str) -> (String, String, String, String) {
    let mut session = String::new();
    let mut date = String::new();
    let parts: Vec<_> = path.split('/').collect();
    for (i, p) in parts.iter().enumerate().take(parts.len().saturating_sub(1)) {
        let (d, _) = folder_details(p);
        if !d.is_empty() {
            date = d;
            session = parts[..=i].join("/");
            break;
        }
    }
    let stem = Path::new(parts.last().unwrap_or(&""))
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"^(.+?)_(\d+)$").unwrap());
    if let Some(c) = re.captures(&stem) {
        (session, date, c[1].into(), c[2].into())
    } else {
        (session, date, String::new(), String::new())
    }
}
pub fn contains_identifier(path: &str, stem: &str) -> bool {
    let p = path.to_lowercase();
    let s = stem.to_lowercase();
    p.match_indices(&s).any(|(i, _)| {
        let before = p[..i].chars().next_back();
        let after = p[i + s.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_alphanumeric())
            && !after.is_some_and(|c| c.is_ascii_alphanumeric())
    })
}
pub fn rebuild(c: &mut Connection) -> Result<()> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    rebuild_on(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
pub fn rebuild_on(tx: &Connection) -> Result<()> {
    let policy = crate::formats::settings(tx)?;
    tx.execute("DELETE FROM matches WHERE method!='manual'", [])
        .map_err(|e| e.to_string())?;
    let all = model::photos(&tx)?;
    let raws: Vec<_> = all
        .iter()
        .filter(|f| f.present && f.kind == "raw")
        .collect();
    let mut index: HashMap<String, Vec<&Photo>> = HashMap::new();
    let mut unusual = Vec::new();
    for raw in &raws {
        if raw.image_number.is_empty() {
            unusual.push(*raw)
        } else {
            index
                .entry(raw.image_number.clone())
                .or_default()
                .push(*raw)
        }
    }
    let digits = Regex::new(r"\d+").unwrap();
    for edit in all.iter().filter(|f| f.present && f.kind == "edited") {
        let mut candidates: HashMap<i64, &Photo> = HashMap::new();
        for token in digits.find_iter(&edit.path) {
            if let Some(rows) = index.get(token.as_str()) {
                for raw in rows {
                    if policy.pair(&raw.name, &edit.name)
                        && contains_identifier(&edit.path, &raw.stem)
                    {
                        candidates.insert(raw.id, raw);
                    }
                }
            }
        }
        for raw in &unusual {
            if policy.pair(&raw.name, &edit.name) && contains_identifier(&edit.path, &raw.stem) {
                candidates.insert(raw.id, raw);
            }
        }
        let local: Vec<_> = candidates
            .values()
            .filter(|r| {
                r.source_id == edit.source_id
                    && !r.session.is_empty()
                    && r.session == edit.session
                    && r.date == edit.date
            })
            .copied()
            .collect();
        let chosen = if local.is_empty() {
            candidates
                .values()
                .filter(|r| r.date.is_empty() || edit.date.is_empty() || r.date == edit.date)
                .copied()
                .collect::<Vec<_>>()
        } else {
            local.clone()
        };
        for raw in chosen {
            let confirmed = local.len() == 1 && local[0].id == raw.id;
            tx.execute(
                "INSERT OR IGNORE INTO matches VALUES(?,?,?,?)",
                params![
                    raw.id,
                    edit.id,
                    if confirmed { "confirmed" } else { "suggested" },
                    if confirmed {
                        "session + identifier"
                    } else {
                        "identifier candidate"
                    }
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    crate::previews::invalidate_related(&tx)?;
    Ok(())
}
pub fn mtime(m: &fs::Metadata) -> Result<String> {
    Ok(m.modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string())
}
fn metadata(path: &Path) -> String {
    let is_video =
        crate::formats::kind(&crate::formats::extension(&path.to_string_lossy())) == Some("video");
    let result = (|| {
        let file = fs::File::open(path).ok()?;
        let exif = exif::Reader::new()
            .read_from_container(&mut std::io::BufReader::new(file))
            .ok()?;
        let mut values = serde_json::Map::new();
        let mut numbers = serde_json::Map::new();
        for field in exif.fields().filter(|f| f.ifd_num == exif::In::PRIMARY) {
            let number = match &field.value {
                exif::Value::Rational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
                exif::Value::Short(v) => v.first().map(|v| *v as f64),
                exif::Value::Long(v) => v.first().map(|v| *v as f64),
                _ => None,
            };
            if let Some(number) = number {
                numbers.insert(field.tag.to_string(), serde_json::json!(number));
            }
            values.insert(
                field.tag.to_string(),
                field.display_value().with_unit(&exif).to_string().into(),
            );
        }
        values.insert("SargeNumeric".into(), serde_json::Value::Object(numbers));
        Some(serde_json::Value::Object(values).to_string())
    })();
    let mut values: serde_json::Value =
        serde_json::from_str(&result.unwrap_or_else(|| "{}".into()))
            .unwrap_or_else(|_| serde_json::json!({}));
    if is_video {
        if let Some(object) = values.as_object_mut() {
            if let Some(v) = crate::video::metadata(path).as_object() {
                object.extend(v.clone());
            }
        }
    }
    if let Ok(info) = fs::metadata(path) {
        if let Ok(time) = info.created() {
            values["FileCreated"] = chrono::DateTime::<chrono::Utc>::from(time)
                .to_rfc3339()
                .into();
        }
    }
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| !is_video && crate::formats::kind(&s.to_ascii_lowercase()) != Some("raw"))
    {
        if let Ok((width, height)) = image::image_dimensions(path) {
            values["ImageWidth"] = width.into();
            values["ImageLength"] = height.into();
        }
    }
    values["SargeMetadataVersion"] = 4.into();
    values.to_string()
}
pub fn scan(
    c: &mut Connection,
    source: &Source,
    job: &str,
    cancel: &AtomicBool,
) -> Result<(i64, Vec<String>)> {
    let base = crate::volumes::canonical(&source.path)?;
    if !crate::volumes::available(&source.path, &source.volume_id) {
        return Err("Source is disconnected or a different volume now occupies this location. Reassociate the source first.".into());
    }
    c.execute(
        "UPDATE sources SET last_complete=NULL WHERE id=?",
        [&source.id],
    )
    .map_err(|e| e.to_string())?;
    let media_policy = crate::formats::settings(c)?;
    let mapping_rules = crate::mappings::rules(c)?;
    let root = base.file_name().unwrap_or_default().to_string_lossy();
    let mut count = 0;
    let mut errors = Vec::new();
    for entry in walkdir::WalkDir::new(&base).follow_links(false) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled; unseen records were retained.".into());
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let ext = p
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if !media_policy.enabled.contains(&ext) {
            continue;
        }
        let Some(kind) = crate::formats::kind(&ext) else {
            continue;
        };
        let info = match fs::metadata(p) {
            Ok(i) => i,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let modified = match mtime(&info) {
            Ok(m) => m,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        let rel = p
            .strip_prefix(&base)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        let (session, date, camera, number) = attributes(&format!("{root}/{rel}"));
        let known: Option<String> = c
            .query_row(
                "SELECT metadata FROM files WHERE source_id=? AND path=? AND size=? AND mtime=?",
                params![source.id, rel, info.len() as i64, modified],
                |r| r.get(0),
            )
            .ok();
        let meta = known
            .filter(|v| {
                serde_json::from_str::<serde_json::Value>(v)
                    .ok()
                    .is_some_and(|v| v["SargeMetadataVersion"] == 4)
            })
            .unwrap_or_else(|| metadata(p));
        c.execute("INSERT INTO files(source_id,path,name,stem,kind,size,mtime,date,session,camera_prefix,image_number,metadata,present,seen) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,1,?) ON CONFLICT(source_id,path) DO UPDATE SET size=excluded.size,mtime=excluded.mtime,date=excluded.date,session=excluded.session,camera_prefix=excluded.camera_prefix,image_number=excluded.image_number,metadata=excluded.metadata,present=1,seen=excluded.seen",params![source.id,rel,name,stem,kind,info.len() as i64,modified,date,session,camera,number,meta,job]).map_err(|e|e.to_string())?;
        c.execute(
            "UPDATE files SET mapped=? WHERE source_id=? AND path=?",
            params![
                crate::mappings::extract(&name, &mapping_rules).to_string(),
                source.id,
                rel
            ],
        )
        .map_err(|e| e.to_string())?;
        count += 1;
        if count % 50 == 0 {
            c.execute("UPDATE jobs SET count=? WHERE id=?", params![count, job])
                .map_err(|e| e.to_string())?;
        }
    }
    // Directory presence and identity are checked again to avoid marking a disconnected disk missing.
    if !crate::volumes::available(&source.path, &source.volume_id) {
        errors.push("Source disconnected during scan; unseen records retained.".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled; unseen records were retained.".into());
    }
    if errors.is_empty() {
        let known = {
            let mut q = c
                .prepare("SELECT id,path FROM files WHERE source_id=? AND seen!=?")
                .map_err(|e| e.to_string())?;
            let rows = q
                .query_map(params![source.id, job], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        for (id, path) in known {
            if media_policy
                .enabled
                .contains(&crate::formats::extension(&path))
            {
                c.execute("UPDATE files SET present=0 WHERE id=?", [id])
                    .map_err(|e| e.to_string())?;
            }
        }
        c.execute(
            "UPDATE sources SET last_complete=? WHERE id=?",
            params![model::stamp(), source.id],
        )
        .map_err(|e| e.to_string())?;
    }
    c.execute(
        "UPDATE sources SET last_scan=? WHERE id=?",
        params![model::stamp(), source.id],
    )
    .map_err(|e| e.to_string())?;
    rebuild(c)?;
    Ok((count, errors))
}
fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
pub fn destinations(
    source: &str,
    matched: &str,
    unmatched: &str,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>)> {
    let s = crate::volumes::canonical(source)?;
    let m = if matched.is_empty() {
        None
    } else {
        Some(crate::volumes::canonical(matched)?)
    };
    let u = if unmatched.is_empty() {
        None
    } else {
        Some(crate::volumes::canonical(unmatched)?)
    };
    if m.is_none() && u.is_none() {
        return Err("Choose at least one copy destination.".into());
    }
    if m.iter().chain(u.iter()).any(|d| overlaps(&s, d))
        || m.as_ref()
            .zip(u.as_ref())
            .is_some_and(|(m, u)| overlaps(m, u))
    {
        return Err("Copy destinations must be separate, outside every selected source, and non-overlapping.".into());
    }
    Ok((s, m, u))
}
pub fn source_destination(root: &str, source: &Source) -> Result<String> {
    if root.is_empty() {
        return Ok(String::new());
    }
    let root = crate::volumes::canonical(root)?;
    let name: String = source
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let id: String = source
        .id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if id.is_empty() {
        return Err("Invalid source identity".into());
    }
    let child = root.join(format!(
        "{}--{}",
        if name.is_empty() { "source" } else { &name },
        id
    ));
    std::fs::create_dir_all(&child).map_err(|e| e.to_string())?;
    let actual = std::fs::canonicalize(child).map_err(|e| e.to_string())?;
    if !actual.starts_with(&root) {
        return Err("Bulk destination escapes through a link".into());
    }
    Ok(actual.to_string_lossy().into_owned())
}
fn digest(path: &Path, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 131072];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().to_vec())
}
pub fn copy_file(
    base: &Path,
    destination: &Path,
    file: &Photo,
    cancel: &AtomicBool,
) -> Result<bool> {
    let rel = Path::new(&file.path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Invalid catalog path".into());
    }
    let source = base.join(rel);
    let actual = fs::canonicalize(&source).map_err(|e| e.to_string())?;
    if !actual.starts_with(base) {
        return Err("Source path now escapes the selected source".into());
    }
    let before = fs::metadata(&source).map_err(|e| e.to_string())?;
    if before.len() as i64 != file.size || mtime(&before)? != file.mtime {
        return Err("Source changed since scan; rescan first".into());
    }
    let target = destination.join(rel);
    let parent = target.parent().ok_or("Invalid destination")?;
    // Reject existing symlinks/junctions before creating any directories.
    let mut check = destination.to_path_buf();
    for part in rel.parent().unwrap_or(Path::new("")).components() {
        check.push(part);
        if check.exists() {
            let actual = fs::canonicalize(&check).map_err(|e| e.to_string())?;
            if !actual.starts_with(destination) {
                return Err("Destination folder escapes through a link".into());
            }
        }
    }
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    if !fs::canonicalize(parent)
        .map_err(|e| e.to_string())?
        .starts_with(destination)
    {
        return Err("Destination escapes through a link".into());
    }
    let source_hash = digest(&source, cancel)?;
    if target.exists() {
        let actual = fs::canonicalize(&target).map_err(|e| e.to_string())?;
        if !actual.starts_with(destination) {
            return Err("Destination file escapes through a link".into());
        }
        if digest(&target, cancel)? == source_hash {
            return Ok(false);
        }
        return Err("Different destination file exists; not overwritten".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut input = fs::File::open(&source).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 131072];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        temp.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let after = fs::metadata(&source).map_err(|e| e.to_string())?;
    if before.len() != after.len()
        || mtime(&before)? != mtime(&after)?
        || digest(temp.path(), cancel)? != source_hash
        || digest(&source, cancel)? != source_hash
    {
        return Err("Source changed during copy; copy discarded".into());
    }
    temp.persist_noclobber(&target)
        .map_err(|e| format!("Destination conflict: {}", e.error))?;
    Ok(true)
}
#[cfg(test)]
pub fn copy(
    c: &Connection,
    source: &Source,
    job: &str,
    matched: &str,
    unmatched: &str,
    cancel: &AtomicBool,
) -> Result<Vec<String>> {
    copy_from_sources(
        c,
        source,
        job,
        matched,
        unmatched,
        cancel,
        &std::collections::HashSet::from([source.id.clone()]),
    )
}
#[cfg(test)]
pub fn copy_from_sources(
    c: &Connection,
    source: &Source,
    job: &str,
    matched: &str,
    unmatched: &str,
    cancel: &AtomicBool,
    allowed_sources: &std::collections::HashSet<String>,
) -> Result<Vec<String>> {
    copy_media_from_sources(
        c,
        source,
        job,
        matched,
        unmatched,
        cancel,
        allowed_sources,
        false,
        false,
    )
}
pub fn copy_media_from_sources(
    c: &Connection,
    source: &Source,
    job: &str,
    matched: &str,
    unmatched: &str,
    cancel: &AtomicBool,
    allowed_sources: &std::collections::HashSet<String>,
    include_videos: bool,
    videos_only: bool,
) -> Result<Vec<String>> {
    if source.last_complete.is_none() {
        return Err("Copy only requires a completed scan first.".into());
    }
    if !crate::volumes::available(&source.path, &source.volume_id) {
        return Err("Source volume is unavailable or has changed.".into());
    }
    let (base, m, u) = destinations(&source.path, matched, unmatched)?;
    let files = model::photos(c)?;
    let policy = crate::formats::settings(c)?;
    let by_id: HashMap<_, _> = files.iter().map(|f| (f.id, f)).collect();
    let mut errors = Vec::new();
    let mut copied = 0;
    let mut identical = 0;
    let mut skipped = 0;
    let mut omitted = 0;
    for file in files.iter().filter(|f| {
        f.source_id == source.id
            && f.present
            && if videos_only {
                f.kind == "video"
            } else {
                f.kind != "video" || include_videos
            }
    }) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled; completed copies remain in place.".into());
        }
        if file.kind != "video" && !policy.matchable(&file.name) {
            omitted += 1;
            c.execute(
                "UPDATE jobs SET omitted=? WHERE id=?",
                params![omitted, job],
            )
            .map_err(|e| e.to_string())?;
            continue;
        }
        let mut q=c.prepare("SELECT raw_id,edited_id,status FROM matches WHERE (raw_id=? OR edited_id=?) AND status!='rejected'").map_err(|e|e.to_string())?;
        let links = q
            .query_map(params![file.id, file.id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let links: Vec<_> = links
            .into_iter()
            .filter(|(r, e, _)| {
                by_id
                    .get(r)
                    .zip(by_id.get(e))
                    .is_some_and(|(r, e)| policy.pair(&r.name, &e.name))
            })
            .collect();
        let unsafe_link = links.iter().any(|(r, e, status)| {
            status != "confirmed"
                || [r, e].iter().any(|id| {
                    by_id
                        .get(id)
                        .is_none_or(|p| !p.present || !allowed_sources.contains(&p.source_id))
                })
        });
        let destination = if links.is_empty() {
            u.as_ref()
        } else {
            m.as_ref()
        };
        if !unsafe_link && destination.is_none() {
            omitted += 1;
        } else if unsafe_link {
            skipped += 1;
            errors.push(format!(
                "{}: ambiguous or outside-source relationship; skipped",
                file.path
            ));
        } else {
            match copy_file(&base, destination.unwrap(), file, cancel) {
                Ok(true) => copied += 1,
                Ok(false) => identical += 1,
                Err(e) => {
                    skipped += 1;
                    errors.push(format!("{}: {e}", file.path));
                }
            }
        }
        c.execute(
            "UPDATE jobs SET copied=?,identical=?,skipped=?,omitted=? WHERE id=?",
            params![copied, identical, skipped, omitted, job],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(errors)
}
