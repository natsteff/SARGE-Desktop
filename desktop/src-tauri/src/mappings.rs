use crate::model::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const FIELDS: &[&str] = &[
    "camera_body",
    "maker",
    "lens",
    "captured",
    "dimensions",
    "iso",
    "aperture",
    "shutter",
    "focal",
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub field: String,
    pub before: String,
    pub after: String,
    pub extensions: Vec<String>,
    pub enabled: bool,
}
pub fn defaults() -> Vec<Rule> {
    vec![Rule {
        id: "default-camera-cr2".into(),
        field: "camera_body".into(),
        before: String::new(),
        after: "_".into(),
        extensions: vec!["cr2".into()],
        enabled: true,
    }]
}
pub fn rules(c: &Connection) -> Result<Vec<Rule>> {
    let value: Option<String> = c
        .query_row(
            "SELECT value FROM settings WHERE key='filename_rules'",
            [],
            |r| r.get(0),
        )
        .ok();
    match value {
        Some(v) => serde_json::from_str(&v).map_err(|e| e.to_string()),
        None => Ok(defaults()),
    }
}
pub fn validate(rules: &[Rule]) -> Result<()> {
    if rules.len() > 100 {
        return Err("Use no more than 100 filename rules.".into());
    }
    let mut ids = std::collections::HashSet::new();
    for r in rules {
        if r.id.is_empty()
            || r.id.len() > 100
            || !ids.insert(&r.id)
            || !FIELDS.contains(&r.field.as_str())
            || r.before.len() > 256
            || r.after.len() > 256
            || r.before.contains(['/', '\\'])
            || r.after.contains(['/', '\\'])
            || r.extensions.is_empty()
            || r.extensions
                .iter()
                .any(|e| !matches!(crate::formats::kind(e), Some("raw" | "edited")))
        {
            return Err("Invalid filename mapping rule.".into());
        }
    }
    Ok(())
}
pub fn extract(name: &str, rules: &[Rule]) -> Value {
    let mut out = serde_json::Map::new();
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return Value::Object(out);
    };
    for r in rules.iter().filter(|r| {
        r.enabled
            && r.extensions
                .iter()
                .any(|e| e.eq_ignore_ascii_case(extension))
    }) {
        if out.contains_key(&r.field) {
            continue;
        }
        let Some(rest) = stem.strip_prefix(&r.before) else {
            continue;
        };
        let value = if r.after.is_empty() {
            rest
        } else if let Some((v, _)) = rest.split_once(&r.after) {
            v
        } else {
            continue;
        };
        if !value.trim().is_empty() {
            out.insert(
                r.field.clone(),
                json!({"value":value.trim(),"rule_id":r.id}),
            );
        }
    }
    Value::Object(out)
}
pub fn reapply(c: &Connection, rules: &[Rule]) -> Result<usize> {
    let mut q = c
        .prepare("SELECT id,name FROM files")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, name) in &rows {
        c.execute(
            "UPDATE files SET mapped=? WHERE id=?",
            params![extract(name, rules).to_string(), id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(rows.len())
}
pub fn save(c: &mut Connection, rules: &[Rule]) -> Result<usize> {
    validate(rules)?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO settings(key,value) VALUES('filename_rules',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(rules).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    let count = reapply(&tx, rules)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(count)
}
pub fn initialize(c: &mut Connection) -> Result<()> {
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM settings WHERE key='filename_rules')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        save(c, &defaults())?;
    }
    Ok(())
}
