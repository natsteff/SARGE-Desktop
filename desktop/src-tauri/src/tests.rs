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
