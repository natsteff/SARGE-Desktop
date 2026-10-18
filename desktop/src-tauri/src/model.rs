use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type Result<T> = std::result::Result<T, String>;
pub fn stamp() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn connection(path: &Path) -> Result<Connection> {
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    c.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    c.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| e.to_string())?;
    Ok(c)
}
pub fn init(path: &Path) -> Result<()> {
    let mut c = connection(path)?;
    c.execute_batch("PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS sources(id TEXT PRIMARY KEY,name TEXT NOT NULL,drive_name TEXT NOT NULL DEFAULT '',description TEXT NOT NULL DEFAULT '',path TEXT NOT NULL,volume_id TEXT NOT NULL DEFAULT '',volume_label TEXT NOT NULL DEFAULT '',drive_letter TEXT NOT NULL DEFAULT '',folder_date TEXT NOT NULL DEFAULT '',archived INTEGER NOT NULL DEFAULT 0,last_scan TEXT,last_complete TEXT);
CREATE TABLE IF NOT EXISTS files(id INTEGER PRIMARY KEY,source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,path TEXT NOT NULL,name TEXT NOT NULL,stem TEXT NOT NULL,kind TEXT NOT NULL,size INTEGER NOT NULL,mtime TEXT NOT NULL,date TEXT NOT NULL,session TEXT NOT NULL,camera_prefix TEXT NOT NULL,image_number TEXT NOT NULL,metadata TEXT NOT NULL DEFAULT '{}',present INTEGER NOT NULL DEFAULT 1,seen TEXT NOT NULL,UNIQUE(source_id,path));
CREATE INDEX IF NOT EXISTS files_source ON files(source_id);
CREATE TABLE IF NOT EXISTS matches(raw_id INTEGER REFERENCES files(id) ON DELETE CASCADE,edited_id INTEGER REFERENCES files(id) ON DELETE CASCADE,status TEXT NOT NULL,method TEXT NOT NULL,PRIMARY KEY(raw_id,edited_id));
CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,source_id TEXT REFERENCES sources(id) ON DELETE CASCADE,name TEXT NOT NULL,mode TEXT NOT NULL,status TEXT NOT NULL,started TEXT NOT NULL,finished TEXT,count INTEGER NOT NULL DEFAULT 0,copied INTEGER NOT NULL DEFAULT 0,identical INTEGER NOT NULL DEFAULT 0,skipped INTEGER NOT NULL DEFAULT 0,errors TEXT NOT NULL DEFAULT '[]');
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS previews(file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,mtime TEXT NOT NULL,size INTEGER NOT NULL,stamp TEXT NOT NULL,data BLOB NOT NULL,origin TEXT NOT NULL);
UPDATE jobs SET status='interrupted',finished=datetime('now') WHERE status IN ('queued','running');").map_err(|e|e.to_string())?;
    let mut q = c
        .prepare("PRAGMA table_info(jobs)")
        .map_err(|e| e.to_string())?;
    let columns = q
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for column in ["omitted", "previews", "unavailable"] {
        if !columns.iter().any(|c| c == column) {
            c.execute(
                &format!("ALTER TABLE jobs ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"),
                [],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if !columns.iter().any(|v| v == "batch_id") {
        c.execute(
            "ALTER TABLE jobs ADD COLUMN batch_id TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if !columns.iter().any(|v| v == "batch_bulk") {
        c.execute(
            "ALTER TABLE jobs ADD COLUMN batch_bulk INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    c.execute_batch("CREATE INDEX IF NOT EXISTS jobs_batch ON jobs(batch_id);")
        .map_err(|e| e.to_string())?;
    let mapped_exists: bool = c
        .prepare("PRAGMA table_info(files)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .any(|v| v == "mapped");
    if !mapped_exists {
        c.execute(
            "ALTER TABLE files ADD COLUMN mapped TEXT NOT NULL DEFAULT '{}'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    drop(q);
    crate::mappings::initialize(&mut c)?;
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub drive_name: String,
    pub description: String,
    pub path: String,
    pub volume_id: String,
    pub volume_label: String,
    pub drive_letter: String,
    pub folder_date: String,
    pub archived: bool,
    pub last_scan: Option<String>,
    pub last_complete: Option<String>,
    #[serde(default)]
    pub online: bool,
}
pub fn sources(c: &Connection) -> Result<Vec<Source>> {
    let mut s=c.prepare("SELECT id,name,drive_name,description,path,volume_id,volume_label,drive_letter,folder_date,archived,last_scan,last_complete FROM sources ORDER BY name COLLATE NOCASE").map_err(|e|e.to_string())?;
    let rows = s
        .query_map([], |r| {
            Ok(Source {
                id: r.get(0)?,
                name: r.get(1)?,
                drive_name: r.get(2)?,
                description: r.get(3)?,
                path: r.get(4)?,
                volume_id: r.get(5)?,
                volume_label: r.get(6)?,
                drive_letter: r.get(7)?,
                folder_date: r.get(8)?,
                archived: r.get(9)?,
                last_scan: r.get(10)?,
                last_complete: r.get(11)?,
                online: false,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
#[derive(Clone, Debug, Serialize)]
pub struct Photo {
    pub id: i64,
    pub source_id: String,
    pub path: String,
    pub name: String,
    pub stem: String,
    pub kind: String,
    pub size: i64,
    pub mtime: String,
    pub date: String,
    pub session: String,
    pub camera_prefix: String,
    pub image_number: String,
    pub metadata: String,
    pub mapped: String,
    pub present: bool,
    pub source_name: String,
    pub matched: bool,
    pub suggested: bool,
    pub preview: bool,
    pub preview_revision: String,
}
pub fn photos(c: &Connection) -> Result<Vec<Photo>> {
    let mut s=c.prepare("SELECT f.id,f.source_id,f.path,f.name,f.stem,f.kind,f.size,f.mtime,f.date,f.session,f.camera_prefix,f.image_number,f.metadata,f.present,s.name,EXISTS(SELECT 1 FROM matches m JOIN files r ON r.id=m.raw_id JOIN files e ON e.id=m.edited_id JOIN sources rs ON rs.id=r.source_id JOIN sources es ON es.id=e.source_id WHERE (m.raw_id=f.id OR m.edited_id=f.id) AND m.status='confirmed' AND r.present AND e.present AND NOT rs.archived AND NOT es.archived),EXISTS(SELECT 1 FROM matches m JOIN files r ON r.id=m.raw_id JOIN files e ON e.id=m.edited_id JOIN sources rs ON rs.id=r.source_id JOIN sources es ON es.id=e.source_id WHERE (m.raw_id=f.id OR m.edited_id=f.id) AND m.status='suggested' AND r.present AND e.present AND NOT rs.archived AND NOT es.archived),EXISTS(SELECT 1 FROM previews p WHERE p.file_id=f.id AND p.mtime=f.mtime AND p.size=f.size) ,coalesce((SELECT p.stamp || ':' || length(p.data) FROM previews p WHERE p.file_id=f.id AND p.mtime=f.mtime AND p.size=f.size),''),f.mapped FROM files f JOIN sources s ON s.id=f.source_id WHERE s.archived=0 ORDER BY f.name COLLATE NOCASE").map_err(|e|e.to_string())?;
    let rows = s
        .query_map([], |r| {
            Ok(Photo {
                id: r.get(0)?,
                source_id: r.get(1)?,
                path: r.get(2)?,
                name: r.get(3)?,
                stem: r.get(4)?,
                kind: r.get(5)?,
                size: r.get(6)?,
                mtime: r.get(7)?,
                date: r.get(8)?,
                session: r.get(9)?,
                camera_prefix: r.get(10)?,
                image_number: r.get(11)?,
                metadata: r.get(12)?,
                present: r.get(13)?,
                source_name: r.get(14)?,
                matched: r.get(15)?,
                suggested: r.get(16)?,
                preview: r.get(17)?,
                preview_revision: r.get(18)?,
                mapped: r.get(19)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn enqueue(
    c: &mut Connection,
    selected: Vec<Source>,
    mode: &str,
    bulk: bool,
) -> Result<Vec<(String, Source)>> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let started = stamp();
    let batch_bulk = bulk || selected.len() > 1;
    let mut jobs = Vec::new();
    for source in selected {
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO jobs(id,source_id,name,mode,status,started,batch_id,batch_bulk) VALUES(?,?,?,?,'queued',?,?,?)",rusqlite::params![id,source.id,source.name,mode,started,batch_id,batch_bulk]).map_err(|e|e.to_string())?;
        jobs.push((id, source));
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(jobs)
}
