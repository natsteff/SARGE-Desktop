use crate::{
    engine,
    model::{self, Result, Source},
    previews, JobRequest,
};
use rusqlite::{params, Connection};
use std::{
    collections::HashSet,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
fn finish(c: &Connection, id: &str, status: &str, errors: &[String]) -> Result<()> {
    c.execute(
        "UPDATE jobs SET status=?,finished=?,errors=? WHERE id=?",
        params![
            status,
            model::stamp(),
            serde_json::to_string(errors).map_err(|e| e.to_string())?,
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn scan(c: &mut Connection, source: &Source, id: &str, cancel: &AtomicBool) -> Result<Vec<String>> {
    c.execute("UPDATE jobs SET status='running' WHERE id=?", [id])
        .map_err(|e| e.to_string())?;
    let (count, errors) = engine::scan(c, source, id, cancel)?;
    c.execute("UPDATE jobs SET count=? WHERE id=?", params![count, id])
        .map_err(|e| e.to_string())?;
    Ok(errors)
}
pub fn run(db: &Path, request: &JobRequest, jobs: Vec<(String, Source)>, cancel: &AtomicBool) {
    let two_phase = request.bulk && request.mode == "scan-copy";
    let mut failed = HashSet::new();
    // Complete every inventory before classifying/copying a bulk scan-and-copy batch.
    if two_phase {
        for (id, source) in &jobs {
            let result = (|| -> Result<Vec<String>> {
                let mut c = model::connection(db)?;
                if cancel.load(Ordering::SeqCst) {
                    return Err("Cancelled before starting".into());
                }
                scan(&mut c, source, id, cancel)
            })();
            match result {
                Ok(errors) if errors.is_empty() => {}
                other => {
                    failed.insert(id.clone());
                    let errors = match other {
                        Ok(mut e) => {
                            e.push("Copy skipped because scan was incomplete.".into());
                            e
                        }
                        Err(e) => vec![e],
                    };
                    if let Ok(c) = model::connection(db) {
                        let _ = finish(
                            &c,
                            id,
                            if cancel.load(Ordering::SeqCst) {
                                "cancelled"
                            } else {
                                "partial"
                            },
                            &errors,
                        );
                    }
                }
            }
        }
    }
    if two_phase {
        if let Ok(mut c) = model::connection(db) {
            let _ = engine::rebuild(&mut c);
        }
    }
    let allowed: HashSet<_> = jobs
        .iter()
        .filter(|(id, _)| !failed.contains(id))
        .map(|(_, s)| s.id.clone())
        .collect();
    for (id, source) in jobs {
        if failed.contains(&id) {
            continue;
        }
        let outcome = (|| -> Result<()> {
            let mut c = model::connection(db)?;
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled before starting".into());
            }
            c.execute("UPDATE jobs SET status='running' WHERE id=?", [&id])
                .map_err(|e| e.to_string())?;
            let mut errors = Vec::new();
            if !two_phase && ["scan", "scan-copy"].contains(&request.mode.as_str()) {
                errors.extend(scan(&mut c, &source, &id, cancel)?);
            }
            let source = model::sources(&c)?
                .into_iter()
                .find(|s| s.id == source.id)
                .ok_or("Source disappeared")?;
            if ["copy", "scan-copy", "copy-videos"].contains(&request.mode.as_str()) {
                if errors.is_empty() {
                    let matched = if request.bulk {
                        engine::source_destination(&request.matched, &source)?
                    } else {
                        request.matched.clone()
                    };
                    let unmatched = if request.bulk {
                        engine::source_destination(&request.unmatched, &source)?
                    } else {
                        request.unmatched.clone()
                    };
                    errors.extend(engine::copy_media_from_sources(
                        &c,
                        &source,
                        &id,
                        &matched,
                        &unmatched,
                        cancel,
                        &allowed,
                        request.include_videos,
                        request.mode == "copy-videos",
                    )?);
                } else {
                    errors.push("Copy skipped because scan was incomplete.".into());
                }
            }
            if !cancel.load(Ordering::SeqCst)
                && ["previews", "scan", "scan-copy"].contains(&request.mode.as_str())
            {
                previews::generate(&c, &source, &id, cancel)?;
            }
            finish(
                &c,
                &id,
                if cancel.load(Ordering::SeqCst) {
                    "cancelled"
                } else if errors.is_empty() {
                    "complete"
                } else {
                    "partial"
                },
                &errors,
            )
        })();
        if let Err(error) = outcome {
            if let Ok(mut c) = model::connection(db) {
                let _ = engine::rebuild(&mut c);
                let _ = finish(
                    &c,
                    &id,
                    if cancel.load(Ordering::SeqCst) {
                        "cancelled"
                    } else {
                        "failed"
                    },
                    &[error],
                );
            }
        }
    }
}
