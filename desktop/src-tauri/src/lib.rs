mod backup;
mod engine;
mod jobs;
mod model;
mod previews;
#[cfg(test)]
mod tests;
mod volumes;
use model::{Result, Source};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

struct Runtime {
    db: PathBuf,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}
#[derive(Default, Deserialize, Clone)]
#[serde(default)]
struct Draft {
    id: String,
    name: String,
    drive_name: String,
    description: String,
    path: String,
    folder_date: String,
    action: String,
    target_id: String,
}
fn normalize(s: &str) -> String {
    s.trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}
fn scope(path: &str) -> String {
    let p = normalize(path);
    p.split_once(':')
        .map(|(_, tail)| tail.trim_start_matches('/').to_string())
        .unwrap_or(p)
}
fn candidate_list(rows: &[Source], draft: &Draft, volume: &volumes::Volume) -> Vec<Source> {
    rows.iter()
        .filter(|s| {
            s.id != draft.id
                && (normalize(&s.name) == normalize(&draft.name)
                    || normalize(&s.path) == normalize(&draft.path)
                    || (!volume.id.is_empty()
                        && volume.id == s.volume_id
                        && scope(&s.path) == scope(&draft.path)))
        })
        .cloned()
        .collect()
}
#[tauri::command]
fn get_state(rt: tauri::State<Runtime>) -> Result<Value> {
    let c = model::connection(&rt.db)?;
    let mut sources = model::sources(&c)?;
    for source in &mut sources {
        source.online = volumes::available(&source.path, &source.volume_id);
    }
    let files = model::photos(&c)?;
    let mut q=c.prepare("SELECT id,source_id,name,mode,status,started,finished,count,copied,identical,skipped,errors,omitted,previews,unavailable FROM jobs ORDER BY started DESC,rowid DESC LIMIT 100").map_err(|e|e.to_string())?;
    let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"source_id":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"mode":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"started":r.get::<_,String>(5)?,"finished":r.get::<_,Option<String>>(6)?,"count":r.get::<_,i64>(7)?,"copied":r.get::<_,i64>(8)?,"identical":r.get::<_,i64>(9)?,"skipped":r.get::<_,i64>(10)?,"errors":r.get::<_,String>(11)?,"omitted":r.get::<_,i64>(12)?,"previews":r.get::<_,i64>(13)?,"unavailable":r.get::<_,i64>(14)?}))).map_err(|e|e.to_string())?.collect::<std::result::Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(
        json!({"inventory_columns":c.query_row("SELECT value FROM settings WHERE key='inventory_columns'",[],|r|r.get::<_,String>(0)).ok(),"sources":sources,"files":files,"jobs":rows,"busy":rt.busy.load(Ordering::SeqCst),"catalog_path":rt.db.to_string_lossy(),"previews_enabled":previews::enabled(&c),"preview_bytes":c.query_row("SELECT coalesce(sum(length(data)),0) FROM previews",[],|r|r.get::<_,i64>(0)).unwrap_or(0)}),
    )
}
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Value> {
    tauri::async_runtime::spawn_blocking(move || {
  match app.dialog().file().blocking_pick_folder(){Some(p)=>{let path=p.into_path().map_err(|e|e.to_string())?;let v=volumes::info(&path);let name=path.file_name().unwrap_or_default().to_string_lossy();let (date,nick)=engine::folder_details(&name);Ok(json!({"path":path.to_string_lossy(),"volume":v,"name":if nick.is_empty(){v.label.clone()}else{nick},"folder_date":date}))},None=>Ok(Value::Null)}
 }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn list_volumes() -> Vec<volumes::Volume> {
    volumes::list()
}
#[tauri::command]
fn source_candidates(rt: tauri::State<Runtime>, draft: Draft) -> Result<Vec<Source>> {
    let c = model::connection(&rt.db)?;
    let volume = volumes::info(std::path::Path::new(&draft.path));
    Ok(candidate_list(&model::sources(&c)?, &draft, &volume))
}
#[tauri::command]
fn save_source(rt: tauri::State<Runtime>, draft: Draft) -> Result<String> {
    if rt.busy.load(Ordering::SeqCst) {
        return Err("Wait for current scan/copy jobs before changing sources.".into());
    }
    if draft.name.trim().is_empty() {
        return Err("Source nickname is required.".into());
    }
    if !draft.folder_date.is_empty() {
        chrono::NaiveDate::parse_from_str(&draft.folder_date, "%Y-%m-%d")
            .map_err(|_| "Invalid folder date")?;
    }
    let c = model::connection(&rt.db)?;
    let rows = model::sources(&c)?;
    let explicit = rows.iter().find(|s| s.id == draft.id);
    let target = if !draft.id.is_empty() {
        Some(explicit.ok_or("Source no longer exists")?)
    } else if draft.action == "update" {
        Some(
            rows.iter()
                .find(|s| s.id == draft.target_id)
                .ok_or("Selected source no longer exists")?,
        )
    } else {
        None
    };
    let unchanged_offline = explicit
        .is_some_and(|s| s.path == draft.path && !std::path::Path::new(&draft.path).is_dir());
    let path = if unchanged_offline {
        PathBuf::from(&draft.path)
    } else {
        volumes::canonical(&draft.path)?
    };
    let volume = if unchanged_offline {
        let s = explicit.unwrap();
        volumes::Volume {
            path: s.path.clone(),
            label: s.volume_label.clone(),
            id: s.volume_id.clone(),
            letter: s.drive_letter.clone(),
            removable: false,
        }
    } else {
        volumes::info(&path)
    };
    let candidates = candidate_list(&rows, &draft, &volume);
    if target.is_none() && !candidates.is_empty() && draft.action != "duplicate" {
        return Err(
            "Existing source found. Choose Update existing or Add as separate source.".into(),
        );
    }
    let id = target
        .map(|s| s.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let preserved = target.filter(|_| draft.id.is_empty());
    let drive_name = if draft.drive_name.trim().is_empty() {
        target.map(|s| s.drive_name.as_str()).unwrap_or("")
    } else {
        draft.drive_name.as_str()
    };
    let folder_date = if draft.folder_date.is_empty() {
        target.map(|s| s.folder_date.as_str()).unwrap_or("")
    } else {
        draft.folder_date.as_str()
    };
    let name = preserved
        .map(|s| s.name.as_str())
        .unwrap_or(draft.name.trim());
    let description = preserved
        .map(|s| s.description.as_str())
        .unwrap_or(&draft.description);
    c.execute("INSERT INTO sources(id,name,drive_name,description,path,volume_id,volume_label,drive_letter,folder_date) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,drive_name=excluded.drive_name,description=excluded.description,path=excluded.path,volume_id=excluded.volume_id,volume_label=excluded.volume_label,drive_letter=excluded.drive_letter,folder_date=excluded.folder_date,archived=0,last_complete=CASE WHEN sources.path!=excluded.path OR sources.volume_id!=excluded.volume_id THEN NULL ELSE sources.last_complete END",params![id,name,drive_name,description,path.to_string_lossy(),volume.id,volume.label,volume.letter,folder_date]).map_err(|e|e.to_string())?;
    Ok(id)
}
#[derive(Clone, Deserialize)]
struct JobRequest {
    source_ids: Vec<String>,
    mode: String,
    #[serde(default)]
    matched: String,
    #[serde(default)]
    unmatched: String,
    #[serde(default)]
    bulk: bool,
    #[serde(default)]
    confirmed_single_category: bool,
}
fn validate_request(request: &JobRequest) -> Result<()> {
    if !["scan", "scan-copy", "copy", "previews"].contains(&request.mode.as_str()) {
        return Err("Invalid operation".into());
    }
    if ["copy", "scan-copy"].contains(&request.mode.as_str()) {
        if request.matched.is_empty() && request.unmatched.is_empty() {
            return Err("Choose at least one copy destination.".into());
        }
        if (request.matched.is_empty() || request.unmatched.is_empty())
            && !request.confirmed_single_category
        {
            return Err("Confirm that files in the omitted category will not be copied.".into());
        }
    }
    Ok(())
}
#[tauri::command]
fn start_jobs(rt: tauri::State<Runtime>, request: JobRequest) -> Result<Value> {
    validate_request(&request)?;
    if rt
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A job is already running.".into());
    }
    let result = (|| {
        let mut c = model::connection(&rt.db)?;
        let rows = model::sources(&c)?;
        let mut selected = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in &request.source_ids {
            if !seen.insert(id) {
                continue;
            }
            let source = rows
                .iter()
                .find(|s| s.id == *id && !s.archived)
                .ok_or("Registered source not found")?;
            if !volumes::available(&source.path, &source.volume_id) {
                return Err(format!("{} is offline or has a different volume identity. Reassociate it before scanning.",source.name));
            }
            if request.mode == "copy" && source.last_complete.is_none() {
                return Err(format!(
                    "{} needs a completed scan before Copy only.",
                    source.name
                ));
            }
            if ["copy", "scan-copy"].contains(&request.mode.as_str()) {
                engine::destinations(&source.path, &request.matched, &request.unmatched)?;
            }
            selected.push(source.clone());
        }
        if selected.is_empty() {
            return Err("Select at least one available registered source.".into());
        }
        if ["copy", "scan-copy"].contains(&request.mode.as_str())
            && selected.len() > 1
            && !request.bulk
        {
            return Err("Confirm the bulk copy operation before starting multiple sources.".into());
        }
        if request.mode == "previews" && !previews::enabled(&c) {
            return Err("Enable stored previews first.".into());
        }
        let tx = c.transaction().map_err(|e| e.to_string())?;
        let mut jobs = Vec::new();
        for source in selected {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO jobs(id,source_id,name,mode,status,started) VALUES(?,?,?,?,'queued',?)",params![id,source.id,source.name,request.mode,model::stamp()]).map_err(|e|e.to_string())?;
            jobs.push((id, source));
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(jobs)
    })();
    let jobs = match result {
        Ok(j) => j,
        Err(e) => {
            rt.busy.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    let count = jobs.len();
    let db = rt.db.clone();
    let busy = rt.busy.clone();
    let cancel = rt.cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        struct Reset(Arc<AtomicBool>);
        impl Drop for Reset {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(busy);
        jobs::run(&db, &request, jobs, &cancel);
    });
    Ok(json!({"queued":count}))
}
#[tauri::command]
fn cancel_jobs(rt: tauri::State<Runtime>) {
    rt.cancel.store(true, Ordering::SeqCst);
}
#[tauri::command]
fn archive_source(rt: tauri::State<Runtime>, id: String, archived: bool) -> Result<()> {
    if rt.busy.load(Ordering::SeqCst) {
        return Err("Wait for current jobs to finish.".into());
    }
    let mut c = model::connection(&rt.db)?;
    c.execute(
        "UPDATE sources SET archived=? WHERE id=?",
        params![archived, id],
    )
    .map_err(|e| e.to_string())?;
    engine::rebuild(&mut c)
}
#[tauri::command]
fn delete_source(rt: tauri::State<Runtime>, id: String, confirmation: String) -> Result<()> {
    if confirmation != "DELETE" || rt.busy.load(Ordering::SeqCst) {
        return Err("Type DELETE and wait for running jobs to finish.".into());
    }
    let mut c = model::connection(&rt.db)?;
    let changed = c
        .execute("DELETE FROM sources WHERE id=? AND archived=1", [id])
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Remove the source from the active list first.".into());
    }
    engine::rebuild(&mut c)
}
#[tauri::command]
fn get_detail(rt: tauri::State<Runtime>, id: i64) -> Result<Value> {
    let c = model::connection(&rt.db)?;
    let mut q=c.prepare("SELECT m.raw_id,m.edited_id,m.status,m.method,f.name,f.path,s.name FROM matches m JOIN files f ON f.id=CASE WHEN m.raw_id=? THEN m.edited_id ELSE m.raw_id END JOIN sources s ON s.id=f.source_id WHERE (m.raw_id=? OR m.edited_id=?) AND s.archived=0 AND f.present=1").map_err(|e|e.to_string())?;
    let rows=q.query_map(params![id,id,id],|r|Ok(json!({"raw_id":r.get::<_,i64>(0)?,"edited_id":r.get::<_,i64>(1)?,"status":r.get::<_,String>(2)?,"method":r.get::<_,String>(3)?,"name":r.get::<_,String>(4)?,"path":r.get::<_,String>(5)?,"drive_name":r.get::<_,String>(6)?}))).map_err(|e|e.to_string())?.collect::<std::result::Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(json!(rows))
}
#[tauri::command]
fn set_match(rt: tauri::State<Runtime>, raw_id: i64, edited_id: i64, status: String) -> Result<()> {
    if rt.busy.load(Ordering::SeqCst) {
        return Err("Wait for scan/copy completion before changing matches.".into());
    }
    if !["confirmed", "rejected"].contains(&status.as_str()) {
        return Err("Invalid match status".into());
    }
    let c = model::connection(&rt.db)?;
    let valid:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM files r JOIN sources rs ON rs.id=r.source_id JOIN files e JOIN sources es ON es.id=e.source_id WHERE r.id=? AND e.id=? AND r.kind='raw' AND e.kind='edited' AND r.present AND e.present AND NOT rs.archived AND NOT es.archived)",params![raw_id,edited_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !valid {
        return Err("Choose a present RAW and export in active sources.".into());
    }
    c.execute("INSERT INTO matches VALUES(?,?,?,'manual') ON CONFLICT(raw_id,edited_id) DO UPDATE SET status=excluded.status,method='manual'",params![raw_id,edited_id,status]).map_err(|e|e.to_string())?;
    previews::invalidate_related(&c)?;
    Ok(())
}
#[tauri::command]
async fn export_csv(app: tauri::AppHandle, rt: tauri::State<'_, Runtime>) -> Result<bool> {
    let db = rt.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(target) = app
            .dialog()
            .file()
            .set_file_name("sarge-inventory.csv")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = target.into_path().map_err(|e| e.to_string())?;
        let c = model::connection(&db)?;
        let mut output =
            "Source,Relative path,Type,Bytes,Folder date,Camera prefix,Matched,Present\r\n"
                .to_string();
        fn cell(s: &str) -> String {
            let s = if s.starts_with(['=', '+', '-', '@']) {
                format!("'{s}")
            } else {
                s.into()
            };
            format!("\"{}\"", s.replace('"', "\"\""))
        }
        for f in model::photos(&c)? {
            output.push_str(&format!(
                "{},{},{},{},{},{},{},{}\r\n",
                cell(&f.source_name),
                cell(&f.path),
                cell(&f.kind),
                f.size,
                cell(&f.date),
                cell(&f.camera_prefix),
                f.matched,
                f.present
            ));
        }
        std::fs::write(path, output).map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn set_previews(rt: tauri::State<Runtime>, enabled: bool) -> Result<()> {
    if rt.busy.load(Ordering::SeqCst) {
        return Err("Wait for current jobs before changing preview settings.".into());
    }
    let c = model::connection(&rt.db)?;
    c.execute("INSERT INTO settings VALUES('previews',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[if enabled{"true"}else{"false"}]).map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
fn clear_previews(rt: tauri::State<Runtime>) -> Result<()> {
    if rt.busy.load(Ordering::SeqCst) {
        return Err("Wait for current jobs before clearing stored previews.".into());
    }
    let c = model::connection(&rt.db)?;
    c.execute("DELETE FROM previews", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn get_previews(rt: tauri::State<Runtime>, ids: Vec<i64>) -> Result<Value> {
    previews::fetch(&model::connection(&rt.db)?, &ids)
}
#[tauri::command]
fn open_photo(rt: tauri::State<Runtime>, id: i64) -> Result<()> {
    let c = model::connection(&rt.db)?;
    let files = model::photos(&c)?;
    let file = files
        .iter()
        .find(|f| f.id == id)
        .ok_or("Photo not found in the active catalog")?;
    let sources = model::sources(&c)?;
    let source = sources
        .iter()
        .find(|s| s.id == file.source_id && !s.archived)
        .ok_or("Source not found")?;
    let path = previews::original(source, file)?;
    open::that(path).map_err(|e| {
        format!("Could not open the original photo in its associated application: {e}")
    })
}
#[tauri::command]
fn save_inventory_columns(rt: tauri::State<Runtime>, columns: Vec<String>) -> Result<()> {
    if columns.len() > 20 || columns.iter().any(|v| v.len() > 40) {
        return Err("Invalid column choices".into());
    }
    let c = model::connection(&rt.db)?;
    c.execute("INSERT INTO settings(key,value) VALUES('inventory_columns',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&columns).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
fn reveal_photo(rt: tauri::State<Runtime>, id: i64) -> Result<()> {
    let c = model::connection(&rt.db)?;
    let files = model::photos(&c)?;
    let file = files.iter().find(|f| f.id == id).ok_or("Photo not found")?;
    let sources = model::sources(&c)?;
    let source = sources
        .iter()
        .find(|s| s.id == file.source_id && !s.archived)
        .ok_or("Source not found")?;
    let path = previews::original(source, file)?;
    #[cfg(target_os = "windows")]
    {
        let canonical = path.to_string_lossy();
        let shell_path = if let Some(unc) = canonical.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", unc)
        } else {
            canonical
                .strip_prefix(r"\\?\")
                .unwrap_or(&canonical)
                .to_string()
        };
        std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(shell_path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        open::that(path.parent().ok_or("Parent folder unavailable")?).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
async fn backup_catalog(
    app: tauri::AppHandle,
    rt: tauri::State<'_, Runtime>,
    theme: String,
) -> Result<bool> {
    if rt
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Wait for running jobs to finish before backing up.".into());
    }
    let busy = rt.busy.clone();
    let db = rt.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        struct Reset(Arc<AtomicBool>);
        impl Drop for Reset {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(busy);
        let Some(target) = app
            .dialog()
            .file()
            .set_file_name(format!(
                "SARGE-backup-{}.tar",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            ))
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = target.into_path().map_err(|e| e.to_string())?;
        if path == db {
            return Err("Choose a separate backup file.".into());
        }
        crate::backup::create(&db, &path, &theme)?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let db = directory.join("catalog.sqlite3");
            model::init(&db).map_err(std::io::Error::other)?;
            app.manage(Runtime {
                db,
                busy: Arc::new(AtomicBool::new(false)),
                cancel: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            pick_folder,
            list_volumes,
            source_candidates,
            save_source,
            start_jobs,
            cancel_jobs,
            archive_source,
            delete_source,
            get_detail,
            set_match,
            export_csv,
            set_previews,
            clear_previews,
            get_previews,
            open_photo,
            reveal_photo,
            backup_catalog,
            save_inventory_columns
        ])
        .run(tauri::generate_context!())
        .expect("SARGE desktop failed to start");
}
