use crate::model::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
pub const RAW: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "sr2", "srf", "dng", "raf", "orf", "rw2", "pef",
    "srw", "x3f", "3fr", "fff", "iiq", "rwl", "erf", "mrw", "mos", "kdc", "dcr",
];
pub const IMAGES: &[&str] = &[
    "jpg", "jpeg", "png", "bmp", "gif", "tif", "tiff", "webp", "heic", "heif", "avif", "jpe",
    "jfif",
];
pub const VIDEOS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "wmv", "webm", "mpg", "mpeg", "mpe", "mts", "m2ts", "3gp",
    "3g2", "vob", "ts", "mxf", "flv", "asf", "m2v", "ogv", "qt", "divx",
];
pub fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}
pub fn kind(ext: &str) -> Option<&'static str> {
    if RAW.contains(&ext) {
        Some("raw")
    } else if IMAGES.contains(&ext) {
        Some("edited")
    } else if VIDEOS.contains(&ext) {
        Some("video")
    } else {
        None
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchRule {
    pub raw: String,
    pub targets: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: Vec<String>,
    pub rules: Vec<MatchRule>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: RAW
                .iter()
                .chain(IMAGES)
                .chain(VIDEOS)
                .map(|s| s.to_string())
                .collect(),
            rules: vec![MatchRule {
                raw: "cr2".into(),
                targets: vec!["jpg".into(), "jpeg".into(), "png".into()],
            }],
        }
    }
}
impl Settings {
    pub fn pair(&self, raw: &str, target: &str) -> bool {
        let (r, t) = (extension(raw), extension(target));
        self.rules
            .iter()
            .any(|rule| rule.raw == r && rule.targets.contains(&t))
    }
    pub fn matchable(&self, name: &str) -> bool {
        let e = extension(name);
        self.rules
            .iter()
            .any(|r| r.raw == e || r.targets.contains(&e))
    }
    pub fn validate(&self) -> Result<()> {
        if self.enabled.len() > 100
            || self.enabled.iter().any(|e| kind(e).is_none())
            || self.rules.len() > RAW.len()
        {
            return Err("Invalid media formats".into());
        }
        let mut seen = std::collections::HashSet::new();
        for r in &self.rules {
            if !RAW.contains(&r.raw.as_str())
                || !seen.insert(&r.raw)
                || r.targets.is_empty()
                || r.targets.len() > IMAGES.len()
                || r.targets.iter().any(|e| !IMAGES.contains(&e.as_str()))
            {
                return Err("Choose RAW-to-image matching pairs; videos cannot be matched.".into());
            }
        }
        Ok(())
    }
}
pub fn settings(c: &Connection) -> Result<Settings> {
    let s: Option<String> = c
        .query_row(
            "SELECT value FROM settings WHERE key='media_formats'",
            [],
            |r| r.get(0),
        )
        .ok();
    s.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .unwrap_or_else(|| Ok(Settings::default()))
}
