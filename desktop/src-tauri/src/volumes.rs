use serde::Serialize;
use std::path::{Path, PathBuf};
#[derive(Clone, Default, Serialize)]
pub struct Volume {
    pub path: String,
    pub label: String,
    pub id: String,
    pub letter: String,
    pub removable: bool,
}
pub fn list() -> Vec<Volume> {
    sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .map(|d| {
            let mut v = info_native(d.mount_point());
            v.path = d.mount_point().to_string_lossy().into_owned();
            if v.label.is_empty() {
                v.label = d.name().to_string_lossy().into_owned();
            }
            v.removable = d.is_removable();
            v
        })
        .collect()
}
pub fn info(path: &Path) -> Volume {
    let mut v = info_native(path);
    if v.path.is_empty() {
        if let Some(d) = list()
            .into_iter()
            .filter(|d| path.starts_with(&d.path))
            .max_by_key(|d| d.path.len())
        {
            return d;
        }
        v.path = path.to_string_lossy().into_owned();
    }
    v
}
pub fn available(path: &str, id: &str) -> bool {
    let p = Path::new(path);
    p.is_dir() && (id.is_empty() || info(p).id == id)
}
pub fn canonical(path: &str) -> Result<PathBuf, String> {
    let p = std::fs::canonicalize(path).map_err(|e| format!("Cannot open folder: {e}"))?;
    if !p.is_dir() {
        return Err("Choose an existing folder or drive.".into());
    }
    Ok(p)
}
#[cfg(not(windows))]
fn info_native(_: &Path) -> Volume {
    Volume::default()
}
#[cfg(windows)]
fn info_native(path: &Path) -> Volume {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumePathNameW(file: *const u16, volume: *mut u16, len: u32) -> i32;
        fn GetVolumeNameForVolumeMountPointW(mount: *const u16, name: *mut u16, len: u32) -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            name: *mut u16,
            len: u32,
            serial: *mut u32,
            max: *mut u32,
            flags: *mut u32,
            fs: *mut u16,
            fs_len: u32,
        ) -> i32;
    }
    fn text(b: &[u16]) -> String {
        String::from_utf16_lossy(&b[..b.iter().position(|x| *x == 0).unwrap_or(b.len())])
    }
    let p: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = vec![0u16; 32768];
    let mut label = vec![0u16; 261];
    let mut id = vec![0u16; 1024];
    let mut serial = 0;
    let mut max = 0;
    let mut flags = 0;
    unsafe {
        if GetVolumePathNameW(p.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
            return Volume::default();
        }
        GetVolumeInformationW(
            root.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut max,
            &mut flags,
            std::ptr::null_mut(),
            0,
        );
        GetVolumeNameForVolumeMountPointW(root.as_ptr(), id.as_mut_ptr(), id.len() as u32);
    }
    let root = text(&root);
    let display = root.strip_prefix("\\\\?\\").unwrap_or(&root).to_string();
    let id = text(&id);
    Volume {
        path: display.clone(),
        label: text(&label),
        id: if id.is_empty() {
            format!("serial:{serial:08X}")
        } else {
            id
        },
        letter: if display.as_bytes().get(1) == Some(&b':') {
            display[..2].into()
        } else {
            String::new()
        },
        removable: false,
    }
}
