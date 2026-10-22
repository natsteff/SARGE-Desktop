use super::{engine, model};
use rusqlite::params;
use std::{fs, path::Path, sync::atomic::AtomicBool};
fn setup() -> (tempfile::TempDir, std::path::PathBuf, model::Source) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("2026_10_17 - Birds");
    fs::create_dir(&root).unwrap();
    let db = dir.path().join("catalog.db");
    model::init(&db).unwrap();
    let c = model::connection(&db).unwrap();
    c.execute(
        "INSERT INTO sources(id,name,path) VALUES('s','Birds',?)",
        [root.to_string_lossy()],
    )
    .unwrap();
    c.execute("INSERT INTO jobs(id,source_id,name,mode,status,started) VALUES('job','s','Birds','scan','running',?)",[model::stamp()]).unwrap();
    let source = model::sources(&c).unwrap().remove(0);
    (dir, db, source)
}
#[test]
fn date_and_identifier_boundaries() {
    assert_eq!(
        engine::folder_details("2026_10_17 - Birds"),
        ("2026-10-17".into(), "Birds".into())
    );
    assert_eq!(engine::folder_details("20260230").0, "");
    assert_eq!(engine::folder_details("202610170").0, "");
    assert!(engine::contains_identifier(
        "edits/1DX_5565 - Bird.jpg",
        "1DX_5565"
    ));
    assert!(engine::contains_identifier(
        "1DX_5565/Web/export.jpg",
        "1DX_5565"
    ));
    assert!(!engine::contains_identifier("1DX_55650.jpg", "1DX_5565"));
    assert_eq!(engine::attributes("20261017/1DX_5565.cr2").2, "1DX");
}
#[test]
fn scan_matches_missing_and_manual_decision_survive() {
    let (_dir, db, source) = setup();
    let root = Path::new(&source.path);
    fs::create_dir(root.join("exports")).unwrap();
    for p in [
        "1DX_1.cr2",
        "1DX_2.cr2",
        "exports/1DX_1 - Eagle.jpg",
        "exports/1DX_10.jpg",
    ] {
        fs::write(root.join(p), b"fixture").unwrap();
    }
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(engine::scan(&mut c, &source, "job", &cancel).unwrap().0, 4);
    let files = model::photos(&c).unwrap();
    assert_eq!(files.iter().filter(|f| f.matched).count(), 2);
    assert_eq!(
        files.iter().find(|f| f.name == "1DX_1.cr2").unwrap().date,
        "2026-10-17"
    );
    let raw = files.iter().find(|f| f.name == "1DX_1.cr2").unwrap().id;
    let edit = files.iter().find(|f| f.name.contains("Eagle")).unwrap().id;
    c.execute(
        "UPDATE matches SET status='rejected',method='manual' WHERE raw_id=? AND edited_id=?",
        params![raw, edit],
    )
    .unwrap();
    fs::remove_file(root.join("1DX_2.cr2")).unwrap();
    engine::scan(&mut c, &source, "job2", &cancel).unwrap();
    let files = model::photos(&c).unwrap();
    assert!(
        !files
            .iter()
            .find(|f| f.name == "1DX_2.cr2")
            .unwrap()
            .present
    );
    assert_eq!(
        files.iter().find(|f| f.name == "1DX_1.cr2").unwrap().id,
        raw
    );
    assert_eq!(
        c.query_row(
            "SELECT status FROM matches WHERE raw_id=? AND edited_id=?",
            params![raw, edit],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "rejected"
    );
}
#[test]
fn copy_preserves_hierarchy_verifies_repeat_and_never_overwrites() {
    let (dir, db, source) = setup();
    let root = Path::new(&source.path);
    fs::create_dir(root.join("edits")).unwrap();
    for p in ["1DX_1.cr2", "1DX_2.cr2", "edits/1DX_1.jpg"] {
        fs::write(root.join(p), b"fixture").unwrap();
    }
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    let source = model::sources(&c).unwrap().remove(0);
    let m = dir.path().join("matched");
    let u = dir.path().join("unmatched");
    fs::create_dir(&m).unwrap();
    fs::create_dir(&u).unwrap();
    assert!(engine::copy(
        &c,
        &source,
        "job",
        m.to_str().unwrap(),
        u.to_str().unwrap(),
        &cancel
    )
    .unwrap()
    .is_empty());
    assert!(m.join("1DX_1.cr2").exists());
    assert!(m.join("edits/1DX_1.jpg").exists());
    assert!(u.join("1DX_2.cr2").exists());
    assert!(engine::copy(
        &c,
        &source,
        "job",
        m.to_str().unwrap(),
        u.to_str().unwrap(),
        &cancel
    )
    .unwrap()
    .is_empty());
    assert_eq!(
        c.query_row("SELECT identical FROM jobs WHERE id='job'", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
    fs::write(u.join("1DX_2.cr2"), b"different").unwrap();
    let errors = engine::copy(
        &c,
        &source,
        "job",
        m.to_str().unwrap(),
        u.to_str().unwrap(),
        &cancel,
    )
    .unwrap();
    assert!(errors.iter().any(|e| e.contains("not overwritten")));
    assert_eq!(fs::read(u.join("1DX_2.cr2")).unwrap(), b"different");
}
#[test]
fn cancellation_retains_unseen_and_copy_rejects_stale_and_overlap() {
    let (dir, db, source) = setup();
    fs::write(Path::new(&source.path).join("1DX_1.cr2"), b"fixture").unwrap();
    let mut c = model::connection(&db).unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    assert!(engine::scan(&mut c, &source, "next", &AtomicBool::new(true)).is_err());
    assert!(model::photos(&c).unwrap()[0].present);
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    let source = model::sources(&c).unwrap().remove(0);
    let m = dir.path().join("m");
    let u = dir.path().join("u");
    fs::create_dir(&m).unwrap();
    fs::create_dir(&u).unwrap();
    assert!(engine::destinations(&source.path, &source.path, u.to_str().unwrap()).is_err());
    fs::write(Path::new(&source.path).join("1DX_1.cr2"), b"new fixture").unwrap();
    let errors = engine::copy(
        &c,
        &source,
        "job",
        m.to_str().unwrap(),
        u.to_str().unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(errors[0].contains("changed since scan"));
    assert_eq!(fs::read_dir(u).unwrap().count(), 0);
}
#[test]
fn same_name_sources_need_choice() {
    let (_dir, db, source) = setup();
    let c = model::connection(&db).unwrap();
    let rows = model::sources(&c).unwrap();
    let draft = super::Draft {
        name: " birds ".into(),
        ..Default::default()
    };
    assert_eq!(
        super::candidate_list(&rows, &draft, &super::volumes::Volume::default()).len(),
        1
    );
    let draft = super::Draft {
        id: source.id,
        ..draft
    };
    assert!(super::candidate_list(&rows, &draft, &super::volumes::Volume::default()).is_empty());
}
#[cfg(unix)]
#[test]
fn destination_symlink_cannot_escape() {
    let (dir, db, source) = setup();
    let root = Path::new(&source.path);
    fs::create_dir(root.join("edits")).unwrap();
    fs::write(root.join("edits/1DX_1.jpg"), b"fixture").unwrap();
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    let target = dir.path().join("target");
    let outside = dir.path().join("outside");
    fs::create_dir(&target).unwrap();
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, target.join("edits")).unwrap();
    let photo = model::photos(&c).unwrap().remove(0);
    assert!(engine::copy_file(&fs::canonicalize(root).unwrap(), &target, &photo, &cancel).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
#[test]
fn ambiguous_relationships_are_not_copied() {
    let (dir, db, source) = setup();
    let root = Path::new(&source.path);
    fs::create_dir(root.join("duplicate")).unwrap();
    for p in ["1DX_1.cr2", "duplicate/1DX_1.cr2", "1DX_1.jpg"] {
        fs::write(root.join(p), b"fixture").unwrap();
    }
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    let source = model::sources(&c).unwrap().remove(0);
    let photos = model::photos(&c).unwrap();
    assert!(photos.iter().all(|p| p.suggested && !p.matched));
    let m = dir.path().join("matched");
    let u = dir.path().join("unmatched");
    fs::create_dir(&m).unwrap();
    fs::create_dir(&u).unwrap();
    let errors = engine::copy(
        &c,
        &source,
        "job",
        m.to_str().unwrap(),
        u.to_str().unwrap(),
        &cancel,
    )
    .unwrap();
    assert_eq!(errors.len(), 3);
    assert_eq!(fs::read_dir(m).unwrap().count(), 0);
    assert_eq!(fs::read_dir(u).unwrap().count(), 0);
}
#[cfg(windows)]
#[test]
fn windows_volume_identity_is_detected_and_mismatch_blocks_scan() {
    let (_dir, _db, source) = setup();
    let path = super::volumes::canonical(&source.path).unwrap();
    let volume = super::volumes::info(&path);
    assert!(
        !volume.id.is_empty(),
        "Expected native Windows volume GUID or serial"
    );
    assert!(!volume.letter.is_empty());
    assert!(super::volumes::available(&source.path, &volume.id));
    assert!(!super::volumes::available(
        &source.path,
        "a-different-volume"
    ));
}

#[test]
fn single_category_requires_confirmation_and_omits_only_unselected_files() {
    let (dir, db, source) = setup();
    let root = Path::new(&source.path);
    for name in ["1DX_1.cr2", "1DX_1.jpg", "1DX_2.cr2"] {
        fs::write(root.join(name), b"fixture").unwrap();
    }
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    let source = model::sources(&c).unwrap().remove(0);
    let dest = dir.path().join("matched");
    fs::create_dir(&dest).unwrap();
    let mut request = super::JobRequest {
        source_ids: vec![source.id.clone()],
        mode: "copy".into(),
        matched: dest.to_string_lossy().into(),
        unmatched: String::new(),
        bulk: false,
        confirmed_single_category: false,
        include_videos: false,
        confirmed_video_copy: false,
    };
    assert!(super::validate_request(&request).is_err());
    request.confirmed_single_category = true;
    super::validate_request(&request).unwrap();
    assert!(
        engine::copy(&c, &source, "job", &request.matched, "", &cancel)
            .unwrap()
            .is_empty()
    );
    assert!(dest.join("1DX_1.cr2").exists());
    assert!(dest.join("1DX_1.jpg").exists());
    assert!(!dest.join("1DX_2.cr2").exists());
    assert_eq!(
        c.query_row("SELECT omitted FROM jobs WHERE id='job'", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let u = dir.path().join("unmatched");
    fs::create_dir(&u).unwrap();
    assert!(
        engine::copy(&c, &source, "job", "", u.to_str().unwrap(), &cancel)
            .unwrap()
            .is_empty()
    );
    assert!(u.join("1DX_2.cr2").exists());
    assert!(!u.join("1DX_1.cr2").exists());
    assert!(engine::destinations(&source.path, "", "").is_err());
}
#[test]
fn bulk_scan_copy_separates_identical_names_and_copy_only_reuses_namespaces() {
    let (dir, db, first) = setup();
    let second_root = dir.path().join("20261018 - More Birds");
    fs::create_dir(&second_root).unwrap();
    fs::write(Path::new(&first.path).join("1DX_1.cr2"), b"first").unwrap();
    fs::write(second_root.join("1DX_1.cr2"), b"second").unwrap();
    let c = model::connection(&db).unwrap();
    c.execute(
        "INSERT INTO sources(id,name,path) VALUES('second','Birds',?)",
        [second_root.to_string_lossy()],
    )
    .unwrap();
    let sources = model::sources(&c).unwrap();
    let destination = dir.path().join("copies");
    fs::create_dir(&destination).unwrap();
    let request = super::JobRequest {
        source_ids: sources.iter().map(|s| s.id.clone()).collect(),
        mode: "scan-copy".into(),
        matched: String::new(),
        unmatched: destination.to_string_lossy().into(),
        bulk: true,
        confirmed_single_category: true,
        include_videos: false,
        confirmed_video_copy: false,
    };
    let make_jobs = |round: &str| -> Vec<(String, model::Source)> {
        model::sources(&c).unwrap().into_iter().map(|s|{let id=format!("{round}-{}",s.id);c.execute("INSERT INTO jobs(id,source_id,name,mode,status,started) VALUES(?,?,?,'scan-copy','queued',?)",params![id,s.id,s.name,model::stamp()]).unwrap();(id,s)}).collect()
    };
    super::jobs::run(&db, &request, make_jobs("first"), &AtomicBool::new(false));
    let a = Path::new(&engine::source_destination(&request.unmatched, &first).unwrap())
        .join("1DX_1.cr2");
    let b = Path::new(
        &engine::source_destination(
            &request.unmatched,
            &sources.iter().find(|s| s.id == "second").unwrap(),
        )
        .unwrap(),
    )
    .join("1DX_1.cr2");
    assert_ne!(a, b);
    assert_eq!(fs::read(a).unwrap(), b"first");
    assert_eq!(fs::read(b).unwrap(), b"second");
    let request = super::JobRequest {
        mode: "copy".into(),
        ..request
    };
    super::jobs::run(&db, &request, make_jobs("repeat"), &AtomicBool::new(false));
    assert_eq!(
        c.query_row(
            "SELECT sum(identical) FROM jobs WHERE id LIKE 'repeat-%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}
#[test]
fn cached_previews_survive_offline_files_and_stale_previews_are_hidden() {
    let (_dir, db, source) = setup();
    let path = Path::new(&source.path).join("IMG_1.png");
    image::RgbImage::from_pixel(320, 240, image::Rgb([30, 60, 90]))
        .save(&path)
        .unwrap();
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    let file = model::photos(&c).unwrap().remove(0);
    assert!(super::previews::fetch(&c, &[file.id])
        .unwrap()
        .as_object()
        .unwrap()
        .is_empty());
    c.execute("INSERT INTO settings VALUES('previews','true')", [])
        .unwrap();
    assert_eq!(
        super::previews::generate(&c, &source, "job", &cancel).unwrap(),
        (1, 0)
    );
    let jpeg: Vec<u8> = c
        .query_row(
            "SELECT data FROM previews WHERE file_id=?",
            [file.id],
            |r| r.get(0),
        )
        .unwrap();
    let img = image::load_from_memory(&jpeg).unwrap();
    assert!(img.width() <= 160 && img.height() <= 160);
    fs::remove_file(path).unwrap();
    assert!(super::previews::fetch(&c, &[file.id])
        .unwrap()
        .get(file.id.to_string())
        .is_some());
    assert!(super::previews::original(&source, &file).is_err());
    c.execute("UPDATE files SET mtime='changed' WHERE id=?", [file.id])
        .unwrap();
    assert!(super::previews::fetch(&c, &[file.id])
        .unwrap()
        .as_object()
        .unwrap()
        .is_empty());
}
#[test]
fn raw_previews_extract_embedded_jpeg_or_use_confirmed_export() {
    let (_dir, db, source) = setup();
    let root = Path::new(&source.path);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&image::RgbImage::from_pixel(
            50,
            40,
            image::Rgb([20, 80, 120]),
        ))
        .unwrap();
    let mut raw = b"II*\0fake CR2".to_vec();
    raw.extend(jpeg);
    fs::write(root.join("1DX_1.cr2"), raw).unwrap();
    fs::write(root.join("1DX_2.cr2"), b"unsupported raw").unwrap();
    image::RgbImage::from_pixel(50, 40, image::Rgb([90, 30, 20]))
        .save(root.join("1DX_2.png"))
        .unwrap();
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    c.execute("INSERT INTO settings VALUES('previews','true')", [])
        .unwrap();
    assert_eq!(
        super::previews::generate(&c, &source, "job", &cancel).unwrap(),
        (3, 0)
    );
    let origins = c
        .prepare("SELECT origin FROM previews ORDER BY file_id")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert!(origins.iter().any(|s| s == "Embedded RAW JPEG"));
    assert!(origins.iter().any(|s| s.starts_with("Matched export:")));
    c.execute("UPDATE matches SET status='rejected',method='manual'", [])
        .unwrap();
    engine::rebuild(&mut c).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM previews WHERE origin LIKE 'Matched export:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM previews WHERE origin='Embedded RAW JPEG'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn alpha_one_database_migration_preserves_catalog_and_adds_preview_settings() {
    let (_dir, db, source) = setup();
    let c = model::connection(&db).unwrap();
    c.execute_batch("DROP TABLE previews;DROP TABLE settings;ALTER TABLE jobs DROP COLUMN omitted;ALTER TABLE jobs DROP COLUMN previews;ALTER TABLE jobs DROP COLUMN unavailable;").unwrap();
    model::init(&db).unwrap();
    model::init(&db).unwrap();
    let c = model::connection(&db).unwrap();
    assert_eq!(model::sources(&c).unwrap()[0].id, source.id);
    assert!(!super::previews::enabled(&c));
    assert_eq!(
        c.query_row(
            "SELECT omitted+previews+unavailable FROM jobs WHERE id='job'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn bulk_copy_includes_confirmed_relationships_between_selected_sources() {
    let (dir, db, raw_source) = setup();
    let edit_root = dir.path().join("exports");
    fs::create_dir(&edit_root).unwrap();
    fs::write(
        Path::new(&raw_source.path).join("1DX_1.cr2"),
        b"raw fixture",
    )
    .unwrap();
    fs::write(edit_root.join("1DX_1.jpg"), b"edit fixture").unwrap();
    let mut c = model::connection(&db).unwrap();
    c.execute(
        "INSERT INTO sources(id,name,path) VALUES('edits','Exports',?)",
        [edit_root.to_string_lossy()],
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    for s in model::sources(&c).unwrap() {
        engine::scan(&mut c, &s, "job", &cancel).unwrap();
    }
    c.execute("UPDATE matches SET status='confirmed',method='manual'", [])
        .unwrap();
    let dest = dir.path().join("matched-all");
    fs::create_dir(&dest).unwrap();
    let sources = model::sources(&c).unwrap();
    let mut jobs = Vec::new();
    for s in &sources {
        let id = format!("copy-{}", s.id);
        c.execute("INSERT INTO jobs(id,source_id,name,mode,status,started) VALUES(?,?,?,'copy','queued',?)",params![id,s.id,s.name,model::stamp()]).unwrap();
        jobs.push((id, s.clone()));
    }
    let request = super::JobRequest {
        source_ids: sources.iter().map(|s| s.id.clone()).collect(),
        mode: "copy".into(),
        matched: dest.to_string_lossy().into(),
        unmatched: String::new(),
        bulk: true,
        confirmed_single_category: true,
        include_videos: false,
        confirmed_video_copy: false,
    };
    super::jobs::run(&db, &request, jobs, &cancel);
    for s in sources {
        let folder = engine::source_destination(&request.matched, &s).unwrap();
        assert!(Path::new(&folder)
            .join(if s.id == "s" {
                "1DX_1.cr2"
            } else {
                "1DX_1.jpg"
            })
            .exists());
    }
    assert_eq!(
        c.query_row(
            "SELECT sum(copied) FROM jobs WHERE id LIKE 'copy-%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn deeply_nested_scan_persists_details_and_matches() {
    let (_dir, db, source) = setup();
    let root = Path::new(&source.path);
    let nested = root.join((0..30).map(|_| "x").collect::<Vec<_>>().join("/"));
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("CAM_123.cr2"), b"raw fixture").unwrap();
    let edits = nested.join("CAM_123").join("exports");
    fs::create_dir_all(&edits).unwrap();
    image::RgbImage::new(12, 8)
        .save(edits.join("final.png"))
        .unwrap();
    let mut c = model::connection(&db).unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    let files = model::photos(&c).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.matched));
    let photo = files.iter().find(|f| f.kind == "edited").unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&photo.metadata).unwrap();
    assert_eq!(metadata["ImageWidth"], 12);
    assert_eq!(metadata["ImageLength"], 8);
    fs::remove_dir_all(root).unwrap();
    assert_eq!(model::photos(&c).unwrap()[0].metadata, files[0].metadata);
}
#[test]
fn backup_contains_consistent_database_previews_preferences_and_manifest() {
    let (dir, db, _) = setup();
    let c = model::connection(&db).unwrap();
    c.execute(
        "INSERT INTO settings VALUES('inventory_columns','[\"iso\"]')",
        [],
    )
    .unwrap();
    let archive = dir.path().join("backup.tar");
    crate::backup::create(&db, &archive, "soldiers").unwrap();
    let bytes = fs::read(&archive).unwrap();
    assert_eq!(&bytes[..14], b"catalog.sqlite");
    let len = usize::from_str_radix(std::str::from_utf8(&bytes[124..135]).unwrap(), 8).unwrap();
    let restored = dir.path().join("restored.sqlite3");
    fs::write(&restored, &bytes[512..512 + len]).unwrap();
    let restored = model::connection(&restored).unwrap();
    assert_eq!(model::sources(&restored).unwrap().len(), 1);
    assert_eq!(
        restored
            .query_row(
                "SELECT value FROM settings WHERE key='inventory_columns'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "[\"iso\"]"
    );
    assert!(bytes.windows(13).any(|v| v == b"manifest.json"));
    assert!(bytes.windows(11).any(|v| v == b"RESTORE.txt"));
}

#[test]
fn filename_rules_default_scope_precedence_and_offline_reapplication() {
    use crate::mappings;
    let (_dir, db, source) = setup();
    let root = Path::new(&source.path);
    fs::write(root.join("1DX_1234.cr2"), b"fixture").unwrap();
    fs::write(root.join("1DX_1234.jpg"), b"fixture").unwrap();
    let mut c = model::connection(&db).unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    let raw = model::photos(&c)
        .unwrap()
        .into_iter()
        .find(|f| f.kind == "raw")
        .unwrap();
    let metadata = raw.metadata.clone();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&raw.mapped).unwrap()["camera_body"]["value"],
        "1DX"
    );
    assert_eq!(
        mappings::extract("1DX_1234.jpg", &mappings::defaults()),
        serde_json::json!({})
    );
    assert_eq!(
        mappings::extract("1DX_1234.CR2", &mappings::defaults())["camera_body"]["value"],
        "1DX"
    );
    assert_eq!(
        mappings::extract("_1234.cr2", &mappings::defaults()),
        serde_json::json!({})
    );
    fs::remove_dir_all(root).unwrap();
    let mut rules = mappings::defaults();
    rules[0].extensions = vec!["cr2".into(), "jpg".into()];
    let mut other = rules[0].clone();
    other.id = "alternate".into();
    other.before = "1DX_".into();
    other.after = String::new();
    rules.push(other.clone());
    assert_eq!(
        mappings::extract("1DX_1234.cr2", &rules)["camera_body"]["value"],
        "1DX"
    );
    rules[0].enabled = false;
    assert_eq!(
        mappings::extract("1DX_1234.cr2", &rules)["camera_body"]["value"],
        "1234"
    );
    mappings::save(&mut c, &rules).unwrap();
    let raw = model::photos(&c)
        .unwrap()
        .into_iter()
        .find(|f| f.kind == "raw")
        .unwrap();
    assert_eq!(raw.metadata, metadata);
    assert!(raw.mapped.contains("1234"));
    assert!(model::photos(&c)
        .unwrap()
        .iter()
        .all(|f| f.mapped.contains("1234")));
    mappings::save(&mut c, &[]).unwrap();
    assert!(model::photos(&c).unwrap().iter().all(|f| f.mapped == "{}"));
    model::init(&db).unwrap();
    assert!(
        mappings::rules(&c).unwrap().is_empty(),
        "Explicit empty settings must survive restart"
    );
    other.extensions = vec!["exe".into()];
    assert!(mappings::validate(&[other]).is_err());
}
#[test]
fn existing_catalog_gets_default_mappings_without_access_to_originals() {
    let (_dir, db, source) = setup();
    let root = Path::new(&source.path);
    fs::write(root.join("R5_0099.cr2"), b"fixture").unwrap();
    let mut c = model::connection(&db).unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    c.execute("DELETE FROM settings WHERE key='filename_rules'", [])
        .unwrap();
    c.execute("UPDATE files SET mapped='{}'", []).unwrap();
    fs::remove_dir_all(root).unwrap();
    model::init(&db).unwrap();
    let photos = model::photos(&c).unwrap();
    assert!(photos[0].mapped.contains("R5"));
    assert!(c.prepare("SELECT batch_id,batch_bulk FROM jobs").is_ok());
}
#[test]
fn bulk_queue_uses_batch_identity_instead_of_matching_timestamps() {
    let (_dir, db, source) = setup();
    let mut c = model::connection(&db).unwrap();
    c.execute(
        "INSERT INTO sources(id,name,path) VALUES('s2','Other',?)",
        [&source.path],
    )
    .unwrap();
    let sources = model::sources(&c).unwrap();
    let first = model::enqueue(&mut c, sources.clone(), "scan", true).unwrap();
    let second = model::enqueue(&mut c, sources, "scan", true).unwrap();
    let batch = |id: &str| {
        c.query_row(
            "SELECT batch_id,batch_bulk FROM jobs WHERE id=?",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)),
        )
        .unwrap()
    };
    assert_eq!(batch(&first[0].0), batch(&first[1].0));
    assert!(batch(&first[0].0).1);
    assert_ne!(batch(&first[0].0).0, batch(&second[0].0).0);
    assert_eq!(
        c.query_row("SELECT batch_id FROM jobs WHERE id='job'", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        ""
    );
}

fn movie_fixture(width: u32, height: u32, seconds: u32) -> Vec<u8> {
    fn atom(kind: &[u8; 4], data: Vec<u8>) -> Vec<u8> {
        let mut b = ((data.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend(kind);
        b.extend(data);
        b
    }
    let mut mvhd = vec![0; 20];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&(seconds * 1000).to_be_bytes());
    let mut tkhd = vec![0; 84];
    tkhd[76..80].copy_from_slice(&(width << 16).to_be_bytes());
    tkhd[80..84].copy_from_slice(&(height << 16).to_be_bytes());
    atom(
        b"moov",
        [atom(b"mvhd", mvhd), atom(b"trak", atom(b"tkhd", tkhd))].concat(),
    )
}
#[test]
fn video_only_scan_persists_metadata_skips_matching_and_uses_safe_video_copy() {
    let (dir, db, mut source) = setup();
    let root = std::path::PathBuf::from(&source.path);
    fs::create_dir_all(root.join("deep/subfolder")).unwrap();
    fs::write(
        root.join("deep/subfolder/clip.MP4"),
        movie_fixture(1920, 1080, 125),
    )
    .unwrap();
    fs::write(root.join("unsupported.mxf"), b"not a decodable container").unwrap();
    let mut c = model::connection(&db).unwrap();
    let cancel = AtomicBool::new(false);
    engine::scan(&mut c, &source, "job", &cancel).unwrap();
    let files = model::photos(&c).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .all(|f| f.kind == "video" && !f.matchable && !f.matched && !f.suggested));
    let meta: serde_json::Value = serde_json::from_str(
        &files
            .iter()
            .find(|f| f.name.ends_with("MP4"))
            .unwrap()
            .metadata,
    )
    .unwrap();
    assert_eq!(meta["DurationSeconds"], 125.0);
    assert_eq!(meta["ImageWidth"], 1920);
    assert_eq!(meta["ImageLength"], 1080);
    c.execute("INSERT INTO settings VALUES('previews','true')", [])
        .unwrap();
    assert_eq!(
        crate::previews::generate(&c, &source, "job", &cancel).unwrap(),
        (0, 0)
    );
    source = model::sources(&c).unwrap().remove(0);
    let dest = dir.path().join("videos");
    fs::create_dir(&dest).unwrap();
    let dest = dest.to_str().unwrap();
    engine::copy_from_sources(
        &c,
        &source,
        "job",
        "",
        dest,
        &cancel,
        &[source.id.clone()].into_iter().collect(),
    )
    .unwrap();
    assert!(!Path::new(dest).join("deep/subfolder/clip.MP4").exists());
    engine::copy_media_from_sources(
        &c,
        &source,
        "job",
        "",
        dest,
        &cancel,
        &[source.id.clone()].into_iter().collect(),
        false,
        true,
    )
    .unwrap();
    assert!(Path::new(dest).join("deep/subfolder/clip.MP4").exists());
    fs::write(
        Path::new(dest).join("unsupported.mxf"),
        b"different existing file",
    )
    .unwrap();
    let errors = engine::copy_media_from_sources(
        &c,
        &source,
        "job",
        "",
        dest,
        &cancel,
        &[source.id.clone()].into_iter().collect(),
        false,
        true,
    )
    .unwrap();
    assert!(!errors.is_empty());
    assert_eq!(
        fs::read(Path::new(dest).join("unsupported.mxf")).unwrap(),
        b"different existing file"
    );
    fs::remove_dir_all(root).unwrap();
    let saved = model::photos(&c).unwrap();
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().any(|f| f.metadata.contains("125")));
}
#[test]
fn broad_formats_default_png_matching_and_configurable_raw_pairs() {
    let (_dir, db, source) = setup();
    let root = Path::new(&source.path);
    for name in [
        "CAM_1.cr2",
        "CAM_1.png",
        "CAM_2.nef",
        "CAM_2.jpg",
        "CAM_3.bmp",
        "clip.mp4",
    ] {
        fs::write(root.join(name), b"fixture").unwrap();
    }
    let mut c = model::connection(&db).unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    let files = model::photos(&c).unwrap();
    assert_eq!(files.len(), 6);
    assert!(
        files
            .iter()
            .find(|f| f.name == "CAM_1.png")
            .unwrap()
            .matched
    );
    assert!(
        !files
            .iter()
            .find(|f| f.name == "CAM_2.nef")
            .unwrap()
            .matchable
    );
    assert!(
        !files
            .iter()
            .find(|f| f.name == "CAM_3.bmp")
            .unwrap()
            .matchable
    );
    let mut policy = crate::formats::Settings::default();
    policy.rules.push(crate::formats::MatchRule {
        raw: "nef".into(),
        targets: vec!["jpg".into()],
    });
    c.execute(
        "INSERT INTO settings VALUES('media_formats',?)",
        [serde_json::to_string(&policy).unwrap()],
    )
    .unwrap();
    engine::rebuild(&mut c).unwrap();
    assert!(
        model::photos(&c)
            .unwrap()
            .iter()
            .find(|f| f.name == "CAM_2.nef")
            .unwrap()
            .matched
    );
    // Keep a manual decision dormant while a pair is disabled, then restore it.
    let files = model::photos(&c).unwrap();
    let raw = files.iter().find(|f| f.name == "CAM_2.nef").unwrap().id;
    let edit = files.iter().find(|f| f.name == "CAM_2.jpg").unwrap().id;
    c.execute(
        "UPDATE matches SET method='manual' WHERE raw_id=? AND edited_id=?",
        params![raw, edit],
    )
    .unwrap();
    policy.rules.clear();
    c.execute(
        "UPDATE settings SET value=? WHERE key='media_formats'",
        [serde_json::to_string(&policy).unwrap()],
    )
    .unwrap();
    engine::rebuild(&mut c).unwrap();
    assert!(model::photos(&c)
        .unwrap()
        .iter()
        .all(|f| !f.matchable && !f.matched));
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM matches WHERE method='manual'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    policy.enabled.retain(|e| e != "bmp");
    c.execute(
        "UPDATE settings SET value=? WHERE key='media_formats'",
        [serde_json::to_string(&policy).unwrap()],
    )
    .unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    assert!(
        model::photos(&c)
            .unwrap()
            .iter()
            .find(|f| f.name == "CAM_3.bmp")
            .unwrap()
            .present
    );
    assert!(crate::formats::RAW
        .iter()
        .all(|e| crate::formats::kind(e) == Some("raw")));
    assert!(crate::formats::VIDEOS
        .iter()
        .all(|e| crate::formats::kind(e) == Some("video")));
    let bad = crate::formats::Settings {
        enabled: vec![],
        rules: vec![crate::formats::MatchRule {
            raw: "cr2".into(),
            targets: vec!["mp4".into()],
        }],
    };
    assert!(bad.validate().is_err());
}
#[test]
fn video_copy_requires_confirmation_and_unmatched_destination() {
    let mut req:super::JobRequest=serde_json::from_value(serde_json::json!({"source_ids":["s"],"mode":"copy","matched":"x","include_videos":true,"confirmed_single_category":true})).unwrap();
    assert!(super::validate_request(&req).is_err());
    req.unmatched = "y".into();
    assert!(super::validate_request(&req).is_ok());
    req.mode = "copy-videos".into();
    assert!(super::validate_request(&req).is_err());
    req.matched.clear();
    req.confirmed_video_copy = true;
    assert!(super::validate_request(&req).is_ok());
}
#[test]
fn native_video_metadata_handles_avi_webm_and_malformed_headers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("movie.avi");
    let mut bytes = b"RIFF\0\0\0\0AVI avih".to_vec();
    bytes.extend(56u32.to_le_bytes());
    let mut avih = vec![0; 56];
    avih[0..4].copy_from_slice(&40000u32.to_le_bytes());
    avih[16..20].copy_from_slice(&250u32.to_le_bytes());
    avih[32..36].copy_from_slice(&1280u32.to_le_bytes());
    avih[36..40].copy_from_slice(&720u32.to_le_bytes());
    bytes.extend(avih);
    fs::write(&path, bytes).unwrap();
    let m = crate::video::metadata(&path);
    assert_eq!(m["DurationSeconds"], 10.0);
    assert_eq!(m["ImageWidth"], 1280);
    fn element(id: &[u8], value: Vec<u8>) -> Vec<u8> {
        assert!(value.len() < 127);
        [id.to_vec(), vec![0x80 | value.len() as u8], value].concat()
    }
    let info = element(
        &[0x15, 0x49, 0xa9, 0x66],
        element(&[0x44, 0x89], 10000.0f64.to_be_bytes().to_vec()),
    );
    let video = element(
        &[0xe0],
        [
            element(&[0xb0], 1920u16.to_be_bytes().to_vec()),
            element(&[0xba], 1080u16.to_be_bytes().to_vec()),
        ]
        .concat(),
    );
    let tracks = element(&[0x16, 0x54, 0xae, 0x6b], element(&[0xae], video));
    let segment = element(&[0x18, 0x53, 0x80, 0x67], [info, tracks].concat());
    let path = dir.path().join("movie.webm");
    fs::write(&path, segment).unwrap();
    let m = crate::video::metadata(&path);
    assert_eq!(m["DurationSeconds"], 10.0);
    assert_eq!(m["ImageLength"], 1080);
    for ext in ["mp4", "mkv", "webm", "avi"] {
        let p = dir.path().join(format!("bad.{ext}"));
        for data in [vec![], vec![0; 32], vec![255; 128], vec![1, 2, 3]] {
            fs::write(&p, data).unwrap();
            let m = crate::video::metadata(&p);
            assert!(m["DurationSeconds"].is_null());
        }
    }
}
#[test]
fn video_copy_job_runs_without_any_photo_match_or_matched_destination() {
    let (dir, db, source) = setup();
    fs::write(
        Path::new(&source.path).join("clip.mov"),
        movie_fixture(640, 480, 8),
    )
    .unwrap();
    let mut c = model::connection(&db).unwrap();
    engine::scan(&mut c, &source, "job", &AtomicBool::new(false)).unwrap();
    let source = model::sources(&c).unwrap().remove(0);
    let dest = dir.path().join("video-copies");
    fs::create_dir(&dest).unwrap();
    let request:super::JobRequest=serde_json::from_value(serde_json::json!({"source_ids":[source.id],"mode":"copy-videos","unmatched":dest,"bulk":true,"confirmed_video_copy":true})).unwrap();
    super::validate_request(&request).unwrap();
    let queued = model::enqueue(&mut c, vec![source.clone()], "copy-videos", true).unwrap();
    let id = queued[0].0.clone();
    super::jobs::run(&db, &request, queued, &AtomicBool::new(false));
    let result: (String, i64, String) = c
        .query_row(
            "SELECT status,copied,errors FROM jobs WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(result, ("complete".into(), 1, "[]".into()));
    let folder = engine::source_destination(dest.to_str().unwrap(), &source).unwrap();
    assert!(Path::new(&folder).join("clip.mov").exists());
}
#[test]
fn movie_metadata_seeks_past_large_media_payload_and_rotates_portrait_dimensions() {
    use std::io::{Seek, SeekFrom, Write};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.mov");
    let mut movie = movie_fixture(1920, 1080, 10);
    let pos = movie.windows(4).position(|w| w == b"tkhd").unwrap() + 4;
    movie[pos + 40..pos + 44].copy_from_slice(&0i32.to_be_bytes());
    movie[pos + 44..pos + 48].copy_from_slice(&65536i32.to_be_bytes());
    let mut f = fs::File::create(&path).unwrap();
    let length = 32 * 1024 * 1024u32;
    f.write_all(&length.to_be_bytes()).unwrap();
    f.write_all(b"mdat").unwrap();
    f.seek(SeekFrom::Start(length as u64)).unwrap();
    f.write_all(&movie).unwrap();
    drop(f);
    let m = crate::video::metadata(&path);
    assert_eq!(m["ImageWidth"], 1080);
    assert_eq!(m["ImageLength"], 1920);
    assert_eq!(m["DurationSeconds"], 10.0);
}
