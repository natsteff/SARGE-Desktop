use crate::model::{self, Result};
use std::{
    io::{Read, Write},
    path::Path,
};

// Portable POSIX ustar archive; fixed, application-owned member names only.
fn stream_member(
    out: &mut impl Write,
    name: &str,
    size: u64,
    mut data: impl Read,
) -> std::io::Result<()> {
    if size > 0o77777777777 {
        return Err(std::io::Error::other(
            "Archive member exceeds supported size",
        ));
    }
    let mut h = [0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[100..108].copy_from_slice(b"0000600\0");
    h[108..116].copy_from_slice(b"0000000\0");
    h[116..124].copy_from_slice(b"0000000\0");
    h[124..136].copy_from_slice(format!("{:011o}\0", size).as_bytes());
    h[136..148].copy_from_slice(b"00000000000\0");
    h[148..156].fill(b' ');
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    let checksum: u32 = h.iter().map(|b| *b as u32).sum();
    h[148..156].copy_from_slice(format!("{:06o}\0 ", checksum).as_bytes());
    out.write_all(&h)?;
    std::io::copy(&mut data, out)?;
    out.write_all(&vec![0; ((512 - size % 512) % 512) as usize])
}
fn member(out: &mut impl Write, name: &str, data: &[u8]) -> std::io::Result<()> {
    stream_member(out, name, data.len() as u64, data)
}
pub fn create(db: &Path, target: &Path, theme: &str) -> Result<()> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let snapshot = directory.path().join("catalog.sqlite3");
    let c = model::connection(db)?;
    // SQLite VACUUM INTO creates a transactionally consistent standalone database,
    // including committed WAL contents, without copying live sidecar files.
    c.execute("VACUUM INTO ?", [snapshot.to_string_lossy().as_ref()])
        .map_err(|e| e.to_string())?;
    let data = std::fs::File::open(snapshot).map_err(|e| e.to_string())?;
    let size = data.metadata().map_err(|e| e.to_string())?.len();
    let mut out = tempfile::NamedTempFile::new_in(target.parent().ok_or("Choose a backup folder")?)
        .map_err(|e| e.to_string())?;
    let manifest=serde_json::json!({"application":"SARGE Desktop","version":env!("CARGO_PKG_VERSION"),"created":model::stamp(),"theme":if theme=="soldiers"{"soldiers"}else{"default"},"photos_included":false}).to_string();
    stream_member(&mut out, "catalog.sqlite3", size, data).map_err(|e| e.to_string())?;
    member(&mut out, "manifest.json", manifest.as_bytes()).map_err(|e| e.to_string())?;
    member(&mut out,"RESTORE.txt",b"SARGE catalog backup. Original photos and copies are not included.\nTo restore: fully close SARGE. Back up the existing app data folder first.\nExtract this archive. Replace catalog.sqlite3 in %APPDATA%\\io.github.natsteff.sarge.desktop with the extracted database. Remove old catalog.sqlite3-wal and catalog.sqlite3-shm only while SARGE is closed, after saving the original folder.\nReopen SARGE. Select the appearance theme recorded in manifest.json. Preview settings and cached previews are stored in the database. Sources may need reassociation if drive paths changed.\nOn macOS use the catalog folder shown at the bottom of the app.\n").map_err(|e|e.to_string())?;
    out.write_all(&[0; 1024]).map_err(|e| e.to_string())?;
    out.as_file().sync_all().map_err(|e| e.to_string())?;
    out.persist(target).map_err(|e| e.to_string())?;
    Ok(())
}
