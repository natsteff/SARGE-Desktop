//! Read-only CR2 selection evidence. Filesystem lock is not proof of camera origin.
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
const LIMIT: usize = 1024 * 1024;
const PM: &str = "http://ns.camerabits.com/photomechanic/1.0/";
fn boolean(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}
fn record(values: &mut Vec<Value>, origin: &str, field: &str, state: Option<bool>) {
    values.push(json!({"source":origin,"field":field,"value":state}));
}
fn xmp(data: &[u8], origin: &str, values: &mut Vec<Value>) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(doc) = roxmltree::Document::parse(text.trim_matches('\0')) else {
        return false;
    };
    for n in doc.descendants().filter(|n| n.is_element()) {
        for a in n.attributes().filter(|a| a.namespace() == Some(PM)) {
            property(a.name(), a.value(), origin, values);
        }
        if n.tag_name().namespace() == Some(PM) {
            property(n.tag_name().name(), n.text().unwrap_or(""), origin, values);
        }
    }
    true
}
fn property(name: &str, text: &str, origin: &str, values: &mut Vec<Value>) {
    if name == "Tagged" {
        record(values, origin, "PhotoMechanic Tagged", boolean(text));
    }
    if name == "Prefs" {
        let first = text.split(':').next().unwrap_or("").trim();
        let state = if first.eq_ignore_ascii_case("Tagged") {
            text.split(':')
                .nth(1)
                .and_then(|s| boolean(s.split(',').next().unwrap_or("")))
        } else if text.split(':').count() == 4 {
            boolean(first)
        } else {
            None
        };
        record(values, origin, "PhotoMechanic Prefs", state);
    }
}
fn block(f: &mut fs::File, offset: u64, size: usize) -> Option<Vec<u8>> {
    if size > LIMIT || offset.checked_add(size as u64)? > f.metadata().ok()?.len() {
        return None;
    }
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut b = vec![0; size];
    f.read_exact(&mut b).ok()?;
    Some(b)
}
fn embedded(f: &mut fs::File, values: &mut Vec<Value>, warnings: &mut Vec<String>) {
    let Some(h) = block(f, 0, 8) else {
        warnings.push("CR2 header unavailable".into());
        return;
    };
    let little = h.starts_with(b"II");
    if !little && !h.starts_with(b"MM") {
        warnings.push("Unsupported CR2/TIFF header".into());
        return;
    }
    let u16n = |b: &[u8]| {
        if little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        }
    };
    let u32n = |b: &[u8]| {
        if little {
            u32::from_le_bytes(b[..4].try_into().unwrap())
        } else {
            u32::from_be_bytes(b[..4].try_into().unwrap())
        }
    };
    if u16n(&h[2..4]) != 42 {
        warnings.push("Unsupported TIFF signature".into());
        return;
    }
    let offset = u32n(&h[4..8]) as u64;
    let Some(count) = block(f, offset, 2) else {
        warnings.push("TIFF directory unavailable".into());
        return;
    };
    let count = u16n(&count) as usize;
    if count > 1024 {
        warnings.push("TIFF directory limit exceeded".into());
        return;
    }
    let Some(entries) = block(f, offset + 2, count * 12) else {
        warnings.push("Invalid TIFF directory".into());
        return;
    };
    for e in entries.chunks_exact(12).filter(|e| u16n(e) == 700) {
        let size = u32n(&e[4..8]) as usize;
        if ![1, 7].contains(&u16n(&e[2..4])) {
            warnings.push("Unsupported embedded XMP type".into());
            continue;
        }
        let data = if size <= 4 {
            Some(e[8..8 + size].to_vec())
        } else {
            block(f, u32n(&e[8..12]) as u64, size)
        };
        if !data.is_some_and(|b| xmp(&b, "embedded XMP", values)) {
            warnings.push("Embedded XMP unreadable or exceeds 1 MiB".into());
        }
    }
}
fn trailer(f: &mut fs::File, values: &mut Vec<Value>, warnings: &mut Vec<String>) {
    let Ok(meta) = f.metadata() else { return };
    if meta.len() < 12 {
        return;
    }
    let Some(footer) = block(f, meta.len() - 12, 12) else {
        return;
    };
    if &footer[4..] != b"cbipcbbl" {
        return;
    }
    let size = u32::from_be_bytes(footer[..4].try_into().unwrap()) as usize;
    let Some(start) = meta.len().checked_sub(12 + size as u64) else {
        warnings.push("Invalid Photo Mechanic trailer size".into());
        return;
    };
    let Some(data) = block(f, start, size) else {
        warnings.push("Photo Mechanic trailer exceeds 1 MiB".into());
        return;
    };
    let mut i = 0;
    while i < data.len() {
        if data[i..].iter().all(|b| *b == 0) {
            break;
        }
        if i + 5 > data.len() || data[i] != 0x1c {
            warnings.push("Malformed Photo Mechanic trailer".into());
            break;
        }
        let record_id = data[i + 1];
        let tag = data[i + 2];
        let n = u16::from_be_bytes([data[i + 3], data[i + 4]]) as usize;
        i += 5;
        if n & 0x8000 != 0 || i + n > data.len() {
            warnings.push("Unsupported Photo Mechanic trailer record".into());
            break;
        }
        if record_id == 2 && tag == 221 {
            record(
                values,
                "Photo Mechanic trailer",
                "PhotoMechanic Tagged",
                if n == 4 {
                    match i32::from_be_bytes(data[i..i + 4].try_into().unwrap()) {
                        0 => Some(false),
                        1 => Some(true),
                        _ => None,
                    }
                } else {
                    None
                },
            );
        }
        i += n;
    }
}
#[derive(Default)]
pub struct Inspector {
    folders: std::collections::HashMap<std::path::PathBuf, Vec<fs::DirEntry>>,
}
impl Inspector {
    pub fn inspect(&mut self, base: &Path, path: &Path) -> Value {
        inspect_cached(base, path, &mut self.folders)
    }
}
pub fn inspect(base: &Path, path: &Path) -> Value {
    Inspector::default().inspect(base, path)
}
fn inspect_cached(
    base: &Path,
    path: &Path,
    folders: &mut std::collections::HashMap<std::path::PathBuf, Vec<fs::DirEntry>>,
) -> Value {
    if crate::formats::extension(&path.to_string_lossy()) != "cr2" {
        return Value::Null;
    }
    let canonical_base = fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let mut values = Vec::new();
    let mut warnings = Vec::<String>::new();
    let mut companions = Vec::new();
    if let Ok(meta) = fs::metadata(path) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            record(
                &mut values,
                "filesystem",
                "Windows Read Only",
                Some(meta.file_attributes() & 1 != 0),
            );
        }
        #[cfg(not(windows))]
        {
            let _ = meta;
            warnings.push("Camera filesystem lock detection is Windows-only; POSIX permissions are not interpreted as camera protection".into());
        }
    } else {
        warnings.push("Filesystem attributes unavailable".into());
    }
    if let Ok(mut f) = fs::File::open(path) {
        embedded(&mut f, &mut values, &mut warnings);
        trailer(&mut f, &mut values, &mut warnings);
    } else {
        warnings.push("CR2 unreadable".into());
    }
    // Only exact sibling names; never join unrelated names from another folder/drive.
    if let (Some(parent), Some(stem), Some(name)) =
        (path.parent(), path.file_stem(), path.file_name())
    {
        let names = [
            format!("{}.xmp", stem.to_string_lossy()),
            format!("{}.xmp", name.to_string_lossy()),
        ];
        let entries = folders.entry(parent.to_path_buf()).or_insert_with(|| {
            fs::read_dir(parent)
                .map(|d| {
                    d.flatten()
                        .filter(|e| {
                            e.path()
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("xmp"))
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        {
            for entry in entries.iter() {
                if !names
                    .iter()
                    .any(|n| entry.file_name().to_string_lossy().eq_ignore_ascii_case(n))
                {
                    continue;
                }
                let p = entry.path();
                if fs::symlink_metadata(&p)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(true)
                    || !fs::canonicalize(&p).is_ok_and(|p| p.starts_with(&canonical_base))
                {
                    warnings.push("Sidecar link/escape rejected".into());
                    continue;
                }
                let Ok(m) = fs::metadata(&p) else {
                    warnings.push("Sidecar attributes unavailable".into());
                    continue;
                };
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                companions.push(json!({"path":rel,"size":m.len(),"mtime":crate::engine::mtime(&m).unwrap_or_default()}));
                if !fs::File::open(&p)
                    .ok()
                    .and_then(|mut f| block(&mut f, 0, m.len().try_into().ok()?))
                    .is_some_and(|b| {
                        xmp(
                            &b,
                            &format!("sidecar {}", p.file_name().unwrap().to_string_lossy()),
                            &mut values,
                        )
                    })
                {
                    warnings.push(format!(
                        "Sidecar unreadable, malformed or exceeds 1 MiB: {}",
                        p.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
        }
    }
    companions.sort_by_key(|v| v["path"].as_str().unwrap_or("").to_string());
    values.sort_by_key(|v| v.to_string());
    warnings.sort();
    let positive = values.iter().any(|v| v["value"] == true);
    let conflict = values
        .iter()
        .filter(|v| {
            v["field"]
                .as_str()
                .unwrap_or("")
                .starts_with("PhotoMechanic")
        })
        .any(|v| v["value"] == true)
        && values
            .iter()
            .filter(|v| {
                v["field"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("PhotoMechanic")
            })
            .any(|v| v["value"] == false);
    json!({"version":1,"important":positive,"state":if positive{"important"}else{"unknown"},"evidence":values,"warnings":warnings,"conflict":conflict,"companions":companions,"caveat":"Read-only does not establish Canon origin. Missing evidence does not establish historical absence of protection."})
}
pub fn important(metadata: &str) -> bool {
    serde_json::from_str::<Value>(metadata)
        .ok()
        .is_some_and(|v| v["SargeImportance"]["important"] == true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(property: &str) -> Vec<u8> {
        format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:p="{PM}" {property}/></rdf:RDF></x:xmpmeta>"#).into_bytes()
    }
    #[test]
    fn xmp_namespace_forms_conflicts_and_legacy_prefs() {
        let mut v = vec![];
        assert!(xmp(
            &packet(r#"p:Tagged="True" p:Prefs="0:2:3:-1""#),
            "fixture",
            &mut v
        ));
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["value"], true);
        assert_eq!(v[1]["value"], false);
        let mut v = vec![];
        assert!(xmp(br#"<r xmlns:p="http://ns.camerabits.com/photomechanic/1.0/"><p:Tagged>1</p:Tagged><p:Prefs>Tagged:1, ColorClass:2, Rating:3, FrameNum:0</p:Prefs></r>"#,"fixture",&mut v));
        assert!(v.iter().all(|v| v["value"] == true));
        let mut v = vec![];
        assert!(xmp(
            br#"<r xmlns:p="urn:wrong" p:Tagged="True"/>"#,
            "fixture",
            &mut v
        ));
        assert!(v.is_empty());
        assert!(!xmp(b"<broken>", "fixture", &mut v));
        assert!(xmp(
            &packet(r#"p:Tagged="garbage" p:Prefs="5:0:0:0""#),
            "fixture",
            &mut v
        ));
        assert!(v.iter().all(|v| v["value"].is_null()));
    }
    #[test]
    fn embedded_tiff_and_signed_trailer_record() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("fixture.CR2");
        let xml = packet(r#"p:Tagged="True""#);
        let mut b = b"II\x2a\0\x08\0\0\0\x01\0".to_vec();
        b.extend_from_slice(&700u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        b.extend_from_slice(&26u32.to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend(xml);
        // IPTC-style record 2, dataset 221, signed big-endian integer 1.
        let trailer = b"\x1c\x02\xdd\0\x04\0\0\0\x01";
        b.extend(trailer);
        b.extend((trailer.len() as u32).to_be_bytes());
        b.extend(b"cbipcbbl");
        fs::write(&p, b).unwrap();
        let v = inspect(d.path(), &p);
        assert_eq!(v["important"], true);
        let evidence = v["evidence"].as_array().unwrap();
        assert!(evidence
            .iter()
            .any(|v| v["source"] == "embedded XMP" && v["value"] == true));
        assert!(evidence
            .iter()
            .any(|v| v["source"] == "Photo Mechanic trailer" && v["value"] == true));
        assert_eq!(fs::metadata(&p).unwrap().len() > 0, true);
    }
    #[test]
    fn sidecar_rescan_without_raw_change_and_unknown_provenance() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("image.CR2");
        fs::write(&p, b"fixture").unwrap();
        let before = fs::read(&p).unwrap();
        assert_eq!(inspect(d.path(), &p)["important"], false);
        fs::write(d.path().join("image.XMP"), packet(r#"p:Tagged="True""#)).unwrap();
        let v = inspect(d.path(), &p);
        assert_eq!(v["important"], true);
        assert_eq!(v["companions"].as_array().unwrap().len(), 1);
        fs::write(d.path().join("image.XMP"), packet(r#"p:Tagged="False""#)).unwrap();
        assert_eq!(inspect(d.path(), &p)["state"], "unknown");
        assert_eq!(fs::read(&p).unwrap(), before);
    }
    #[cfg(windows)]
    #[test]
    fn native_readonly_is_independent_from_pm_tag() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("image.cr2");
        fs::write(&p, b"fixture").unwrap();
        let mut permissions = fs::metadata(&p).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&p, permissions).unwrap();
        assert_eq!(inspect(d.path(), &p)["important"], true);
        let mut permissions = fs::metadata(&p).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&p, permissions).unwrap();
        assert_eq!(inspect(d.path(), &p)["important"], false);
    }
}
