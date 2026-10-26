//! Bounded public release notifications. No download/install code or catalog data in requests.
use crate::model::{self, Result};
use reqwest::{blocking::Client, redirect::Policy, Url};
use rusqlite::Connection;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering as AO},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const API: &str = "https://api.github.com/repos/natsteff/SARGE-Desktop/releases";
const DAY: u64 = 86400;
const MAX_PAGES: usize = 3;
const MAX_BODY: usize = 1024 * 1024;
const AUTO: &str = "updates_auto_enabled";
const DISMISSED: &str = "updates_dismissed";
const CACHE: &str = "updates_cache_v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub version: String,
    pub build: Option<u64>,
    pub source_revision: String,
    pub source_modified: Option<bool>,
    pub development: bool,
    pub platform: String,
}
pub fn identity() -> Identity {
    let modified = match env!("SARGE_DIRTY") {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    };
    Identity {
        version: env!("CARGO_PKG_VERSION").into(),
        build: env!("SARGE_BUILD").parse().ok(),
        source_revision: env!("SARGE_SOURCE").into(),
        source_modified: modified,
        development: cfg!(debug_assertions) || modified == Some(true),
        platform: format!("{}_{}", std::env::consts::OS, std::env::consts::ARCH),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub build: Option<u64>,
    pub url: String,
    pub installer_available: bool,
    pub key: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Outcome {
    pub status: String,
    pub installed: Identity,
    pub release: Option<ReleaseInfo>,
    pub message: String,
    pub automatic: bool,
}
impl Outcome {
    fn new(
        status: &str,
        id: &Identity,
        release: Option<ReleaseInfo>,
        message: &str,
        automatic: bool,
    ) -> Self {
        Self {
            status: status.into(),
            installed: id.clone(),
            release,
            message: message.into(),
            automatic,
        }
    }
}
#[derive(Default, Clone, Serialize, Deserialize)]
struct Page {
    etag: Option<String>,
    body: String,
}
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Cache {
    last_attempt: u64,
    last_success: u64,
    backoff_until: u64,
    pages: Vec<Page>,
    complete: bool,
}
#[derive(Serialize)]
pub struct Status {
    pub identity: Identity,
    pub automatic_enabled: bool,
    pub last_attempt: u64,
    pub last_success: u64,
    pub backoff_until: u64,
}
fn get(c: &Connection, key: &str) -> Option<String> {
    c.query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
        r.get(0)
    })
    .ok()
}
fn put(c: &Connection, key: &str, value: &str) -> Result<()> {
    c.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[key,value]).map_err(|e|e.to_string())?;
    Ok(())
}
fn enabled(c: &Connection) -> bool {
    get(c, AUTO).as_deref() != Some("false")
}
pub fn set_automatic(c: &Connection, value: bool) -> Result<()> {
    put(c, AUTO, if value { "true" } else { "false" })
}
fn dismissed(c: &Connection) -> Vec<String> {
    get(c, DISMISSED)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
pub fn dismiss(c: &Connection, key: &str) -> Result<()> {
    if key.len() > 256 {
        return Err("Invalid release identity.".into());
    }
    let mut keys = dismissed(c);
    keys.retain(|k| k != key);
    keys.push(key.to_owned());
    if keys.len() > 128 {
        keys.drain(..keys.len() - 128);
    }
    put(
        c,
        DISMISSED,
        &serde_json::to_string(&keys).map_err(|e| e.to_string())?,
    )
}
fn load(c: &Connection) -> Cache {
    get(c, CACHE)
        .filter(|s| s.len() <= MAX_BODY * MAX_PAGES * 2 + 8192)
        .and_then(|s| serde_json::from_str::<Cache>(&s).ok())
        .filter(|s| s.pages.len() <= MAX_PAGES && s.pages.iter().all(|p| p.body.len() <= MAX_BODY))
        .unwrap_or_default()
}
fn save(c: &Connection, cache: &Cache) -> Result<()> {
    put(
        c,
        CACHE,
        &serde_json::to_string(cache).map_err(|e| e.to_string())?,
    )
}
pub fn status(c: &Connection) -> Status {
    let s = load(c);
    Status {
        identity: identity(),
        automatic_enabled: enabled(c),
        last_attempt: s.last_attempt,
        last_success: s.last_success,
        backoff_until: s.backoff_until,
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    state: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Release {
    id: u64,
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    assets: Vec<Asset>,
}
#[derive(Clone)]
struct Candidate {
    version: Version,
    info: ReleaseInfo,
}
fn version(tag: &str) -> Option<Version> {
    let v = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    if !v.pre.is_empty() {
        let p = v.pre.as_str().split('.').collect::<Vec<_>>();
        if p.len() < 2
            || !["alpha", "beta", "rc"].contains(&p[0])
            || !p[1..]
                .iter()
                .all(|part| part.bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
    }
    Some(v)
}
fn exact_url(value: &str, expected: &str) -> bool {
    let Ok(u) = Url::parse(value) else {
        return false;
    };
    u.scheme() == "https"
        && u.host_str() == Some("github.com")
        && u.username().is_empty()
        && u.password().is_none()
        && u.port().is_none()
        && u.query().is_none()
        && u.fragment().is_none()
        && Url::parse(expected).ok().as_ref() == Some(&u)
}
pub fn validate_release_url(value: &str) -> bool {
    let Ok(u) = Url::parse(value) else {
        return false;
    };
    let prefix = "/natsteff/SARGE-Desktop/releases/tag/";
    let Some(tag) = u.path().strip_prefix(prefix) else {
        return false;
    };
    version(tag).is_some() && exact_url(value, &format!("https://github.com{prefix}{tag}"))
}
#[derive(Deserialize)]
struct Metadata {
    schema: u64,
    version: String,
    build: u64,
}
fn build(body: Option<&str>, v: &Version) -> Option<u64> {
    let body = body?;
    let marker = "<!-- sarge-release: ";
    if body.matches(marker).count() != 1 {
        return None;
    }
    let value = body.split_once(marker)?.1.split_once(" -->")?.0;
    let m: Metadata = serde_json::from_str(value).ok()?;
    (m.schema == 1 && m.version == v.to_string() && m.build > 0).then_some(m.build)
}
fn installer(r: &Release, v: &Version, platform: &str) -> bool {
    let suffix = match platform {
        "windows_x86_64" => "x64-setup.exe",
        "macos_aarch64" => "aarch64.dmg",
        "macos_x86_64" => "x64.dmg",
        _ => return false,
    };
    r.assets.iter().any(|a| {
        a.state == "uploaded"
            && a.size > 0
            && [
                format!("SARGE.Desktop_{v}_{suffix}"),
                format!("SARGE Desktop_{v}_{suffix}"),
            ]
            .contains(&a.name)
            && exact_url(
                &a.browser_download_url,
                &format!(
                    "https://github.com/natsteff/SARGE-Desktop/releases/download/{}/{}",
                    r.tag_name, a.name
                ),
            )
    })
}
fn candidates(releases: &[Release], id: &Identity) -> Vec<Candidate> {
    let Some(current) = version(&id.version) else {
        return vec![];
    };
    releases
        .iter()
        .filter_map(|r| {
            let v = version(&r.tag_name)?;
            if r.draft
                || r.id == 0
                || r.prerelease != !v.pre.is_empty()
                || (current.pre.is_empty() && !v.pre.is_empty())
                || !validate_release_url(&r.html_url)
                || !exact_url(
                    &r.html_url,
                    &format!(
                        "https://github.com/natsteff/SARGE-Desktop/releases/tag/{}",
                        r.tag_name
                    ),
                )
            {
                return None;
            }
            let b = build(r.body.as_deref(), &v);
            Some(Candidate {
                info: ReleaseInfo {
                    version: v.to_string(),
                    build: b,
                    url: r.html_url.clone(),
                    installer_available: installer(r, &v, &id.platform),
                    key: format!(
                        "{}:{}",
                        r.tag_name,
                        b.map(|n| n.to_string()).unwrap_or_else(|| "unknown".into())
                    ),
                },
                version: v,
            })
        })
        .collect()
}
fn compare(c: &Candidate, id: &Identity, v: &Version) -> Option<Ordering> {
    match c.version.cmp_precedence(v) {
        Ordering::Equal => match (c.info.build, id.build) {
            (Some(a), Some(b)) => Some(a.cmp(&b)),
            _ => None,
        },
        other => Some(other),
    }
}
fn select(releases: &[Release], complete: bool, id: &Identity, auto: bool) -> Outcome {
    let Some(current) = version(&id.version) else {
        return Outcome::new(
            "unknown",
            id,
            None,
            "The installed version cannot be compared safely.",
            auto,
        );
    };
    let mut items = candidates(releases, id);
    // None is only a deterministic listing position, never evidence of an older build.
    items.sort_by(|a, b| {
        b.version
            .cmp_precedence(&a.version)
            .then_with(|| b.info.build.cmp(&a.info.build))
    });
    for item in &items {
        if item.info.installer_available && compare(item, id, &current) == Some(Ordering::Greater) {
            return Outcome::new("newer",id,Some(item.info.clone()),"A newer release has a suitable installer. Open its official page to review and install it yourself.",auto);
        }
    }
    if let Some(item) = items
        .iter()
        .find(|c| compare(c, id, &current) == Some(Ordering::Greater))
    {
        return Outcome::new(
            "source_only",
            id,
            Some(item.info.clone()),
            "A newer release is published, but it has no suitable installer for this platform.",
            auto,
        );
    }
    let latest = items.first().map(|c| c.info.clone());
    if !complete {
        return Outcome::new(
            "unknown",
            id,
            latest,
            "The bounded release listing is incomplete; a current-release claim cannot be made.",
            auto,
        );
    }
    if id.development {
        return Outcome::new("unknown",id,latest,"This is a development or modified build. Its correspondence to a published installer is not confirmed.",auto);
    }
    if let Some(item) = items.first() {
        if item.version.cmp_precedence(&current) == Ordering::Equal
            && !item.info.installer_available
        {
            return Outcome::new(
                "source_only",
                id,
                latest,
                "The release has no suitable installer for this platform.",
                auto,
            );
        }
        if items.iter().any(|c| {
            c.version.cmp_precedence(&current) == Ordering::Equal && c.info.build.is_none()
        }) || id.build.is_none()
        {
            return Outcome::new("unknown",id,latest,"This release or installed app has no comparable build metadata. Version is known; build equivalence is not.",auto);
        }
        if compare(item, id, &current) == Some(Ordering::Equal) {
            return Outcome::new(
                "current",
                id,
                latest,
                "The installed version and build match the newest eligible release found.",
                auto,
            );
        }
    }
    Outcome::new(
        "unknown",
        id,
        latest,
        "No matching published version and build could be confirmed.",
        auto,
    )
}
struct Reply {
    status: u16,
    body: String,
    etag: Option<String>,
    retry_after: Option<String>,
    remaining: Option<String>,
    reset: Option<String>,
}
trait Transport {
    fn page(&mut self, page: usize, etag: Option<&str>, timeout: Duration) -> Result<Reply>;
}
struct Http(Client);
impl Transport for Http {
    fn page(&mut self, page: usize, etag: Option<&str>, timeout: Duration) -> Result<Reply> {
        let mut request = self
            .0
            .get(format!("{API}?per_page=100&page={page}"))
            .timeout(timeout);
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag)
        }
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                "The update check timed out. Try again later.".to_owned()
            } else {
                "GitHub could not be reached. Check your connection and try again.".to_owned()
            }
        })?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned)
        };
        let mut reply = Reply {
            status: response.status().as_u16(),
            body: String::new(),
            etag: header("etag"),
            retry_after: header("retry-after"),
            remaining: header("x-ratelimit-remaining"),
            reset: header("x-ratelimit-reset"),
        };
        if reply.status == 200 {
            if response
                .content_length()
                .is_some_and(|n| n > MAX_BODY as u64)
            {
                return Err("GitHub returned an oversized release listing.".into());
            }
            let mut bytes = vec![];
            response
                .take((MAX_BODY + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| "GitHub's response could not be read or timed out.".to_owned())?;
            if bytes.len() > MAX_BODY {
                return Err("GitHub returned an oversized release listing.".into());
            }
            reply.body = String::from_utf8(bytes)
                .map_err(|_| "GitHub returned invalid release text.".to_owned())?;
        }
        Ok(reply)
    }
}
fn backoff(r: &Reply, time: u64) -> u64 {
    let retry = r
        .retry_after
        .as_deref()
        .and_then(|s| {
            s.parse::<u64>()
                .ok()
                .map(|n| time.saturating_add(n))
                .or_else(|| {
                    chrono::DateTime::parse_from_rfc2822(s)
                        .ok()
                        .and_then(|d| u64::try_from(d.timestamp()).ok())
                })
        })
        .unwrap_or(0);
    let reset = if r.remaining.as_deref() == Some("0") {
        r.reset
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    } else {
        0
    };
    time.saturating_add(60).max(retry).max(reset)
}
fn fetch(
    t: &mut impl Transport,
    old: &Cache,
    time: u64,
) -> std::result::Result<Cache, (String, u64)> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut cache = Cache {
        last_attempt: time,
        ..Cache::default()
    };
    for page in 1..=MAX_PAGES {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| ("The update check timed out.".into(), 0))?;
        let previous = old.pages.get(page - 1);
        let r = t
            .page(page, previous.and_then(|p| p.etag.as_deref()), timeout)
            .map_err(|e| (e, 0))?;
        if r.status == 403 || r.status == 429 {
            return Err(("GitHub has limited update requests. Try again after the cooldown shown in Settings.".into(),backoff(&r,time)));
        }
        let exhausted = r.remaining.as_deref() == Some("0");
        let pause = if exhausted { backoff(&r, time) } else { 0 };
        let stored = match r.status {
            304 => previous.cloned().ok_or_else(|| {
                (
                    "GitHub returned a cache response without a saved listing.".into(),
                    0,
                )
            })?,
            200 => Page {
                etag: r.etag,
                body: r.body,
            },
            code => {
                return Err((
                    format!("GitHub returned HTTP {code}. No update result was confirmed."),
                    0,
                ))
            }
        };
        let entries: Vec<Release> = serde_json::from_str(&stored.body)
            .map_err(|_| ("GitHub returned a malformed release listing.".into(), 0))?;
        if entries.len() > 100 {
            return Err(("GitHub exceeded the requested page limit.".into(), 0));
        }
        let last = entries.len() < 100;
        cache.pages.push(stored);
        cache.backoff_until = cache.backoff_until.max(pause);
        if last {
            cache.complete = true;
            break;
        }
        if exhausted {
            return Err((
                "GitHub's request allowance was exhausted before the listing finished.".into(),
                pause,
            ));
        }
    }
    cache.last_success = time;
    Ok(cache)
}
fn releases(cache: &Cache) -> Result<Vec<Release>> {
    let mut all = vec![];
    for page in &cache.pages {
        all.extend(
            serde_json::from_str::<Vec<Release>>(&page.body)
                .map_err(|_| "The saved release listing is invalid.".to_owned())?,
        )
    }
    Ok(all)
}
fn check_with(
    c: &Connection,
    manual: bool,
    time: u64,
    id: &Identity,
    t: &mut impl Transport,
) -> Result<Outcome> {
    let auto = !manual;
    let mut cache = load(c);
    if auto
        && (!enabled(c)
            || (cache.last_attempt > 0 && time.saturating_sub(cache.last_attempt) < DAY))
    {
        return Ok(Outcome::new(
            "skipped",
            id,
            None,
            "Automatic checks are disabled or not due.",
            true,
        ));
    }
    if time < cache.backoff_until {
        return Ok(Outcome::new(
            "failure",
            id,
            None,
            "GitHub's update-check cooldown is active. Try again after the time shown in Settings.",
            auto,
        ));
    }
    if !(manual && cache.last_success > 0 && time.saturating_sub(cache.last_success) < 60) {
        cache.last_attempt = time;
        save(c, &cache)?;
        match fetch(t, &cache, time) {
            Ok(next) => {
                cache = next;
                save(c, &cache)?
            }
            Err((message, until)) => {
                cache.backoff_until = until;
                save(c, &cache)?;
                return Ok(Outcome::new("failure", id, None, &message, auto));
            }
        }
    }
    let mut result = select(&releases(&cache)?, cache.complete, id, auto);
    if auto
        && (!enabled(c)
            || result
                .release
                .as_ref()
                .is_some_and(|r| dismissed(c).contains(&r.key)))
    {
        result.status = "skipped".into();
        result.release = None
    }
    Ok(result)
}
pub struct Service {
    db: PathBuf,
    in_flight: AtomicBool,
}
impl Service {
    pub fn new(db: PathBuf) -> Self {
        Self {
            db,
            in_flight: AtomicBool::new(false),
        }
    }
    pub fn check(&self, manual: bool) -> Result<Outcome> {
        if self
            .in_flight
            .compare_exchange(false, true, AO::SeqCst, AO::SeqCst)
            .is_err()
        {
            return Ok(Outcome::new(
                "failure",
                &identity(),
                None,
                "An update check is already in progress. Try again shortly.",
                !manual,
            ));
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, AO::SeqCst)
            }
        }
        let _reset = Reset(&self.in_flight);
        let c = model::connection(&self.db)?;
        let client = Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(Policy::none())
            .referer(false)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .retry(reqwest::retry::never())
            .user_agent("SARGE-Desktop-Update-Check")
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    "Accept",
                    reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
                );
                h.insert(
                    "X-GitHub-Api-Version",
                    reqwest::header::HeaderValue::from_static("2022-11-28"),
                );
                h
            })
            .build()
            .map_err(|_| "Could not initialize a secure update connection.".to_owned())?;
        check_with(&c, manual, now(), &identity(), &mut Http(client))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn id(v: &str, b: Option<u64>) -> Identity {
        Identity {
            version: v.into(),
            build: b,
            source_revision: "test".into(),
            source_modified: Some(false),
            development: false,
            platform: "windows_x86_64".into(),
        }
    }
    fn release(v: &str, b: Option<u64>, binary: bool) -> Release {
        serde_json::from_value(json!({"id":1,"tag_name":format!("v{v}"),"html_url":format!("https://github.com/natsteff/SARGE-Desktop/releases/tag/v{v}"),"draft":false,"prerelease":v.contains('-'),"body":b.map(|b|format!("<!-- sarge-release: {{\"schema\":1,\"version\":\"{v}\",\"build\":{b}}} -->")),"assets":if binary{vec![json!({"name":format!("SARGE.Desktop_{v}_x64-setup.exe"),"browser_download_url":format!("https://github.com/natsteff/SARGE-Desktop/releases/download/v{v}/SARGE.Desktop_{v}_x64-setup.exe"),"size":8000,"state":"uploaded"})]}else{vec![]}})).unwrap()
    }
    struct Mock {
        replies: Vec<Reply>,
        calls: usize,
        etags: Vec<Option<String>>,
    }
    impl Transport for Mock {
        fn page(&mut self, _: usize, etag: Option<&str>, _: Duration) -> Result<Reply> {
            self.calls += 1;
            self.etags.push(etag.map(str::to_owned));
            if self.replies.is_empty() {
                return Err("Offline fixture".into());
            }
            Ok(self.replies.remove(0))
        }
    }
    fn reply(status: u16, body: &str) -> Reply {
        Reply {
            status,
            body: body.into(),
            etag: Some("\"fixture\"".into()),
            retry_after: None,
            remaining: None,
            reset: None,
        }
    }
    fn mock(replies: Vec<Reply>) -> Mock {
        Mock {
            replies,
            calls: 0,
            etags: vec![],
        }
    }
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);")
            .unwrap();
        c
    }
    #[test]
    fn historical_and_numeric_prerelease_ordering() {
        for (a, b) in [
            ("0.3.0-beta.1", "0.3.0-beta.2"),
            ("0.3.0-beta.7", "0.3.0-beta.8"),
            ("0.3.0-beta.8", "0.3.0-beta.10"),
            ("0.3.0-beta.7.2", "0.3.0-beta.7.10"),
            ("0.3.0-beta.10", "0.3.0"),
            ("0.3.0", "0.4.0"),
        ] {
            assert_eq!(
                select(&[release(b, None, true)], true, &id(a, Some(99)), false).status,
                "newer",
                "{a} < {b}"
            )
        }
        assert!(version("v0.3.0-beta.7").is_some());
        for bad in [
            "v0.3",
            "build-7",
            "v0.3.0-beta.07",
            "v0.3.0-nightly.2",
            "v0.3.0-beta.7.extra",
        ] {
            assert!(version(bad).is_none())
        }
    }
    #[test]
    fn stable_channel_promotion_and_build_reset() {
        assert_eq!(
            select(
                &[
                    release("0.3.0", Some(1), true),
                    release("0.4.0-beta.1", Some(1), true)
                ],
                true,
                &id("0.3.0", Some(1)),
                false
            )
            .status,
            "current"
        );
        assert_eq!(
            select(
                &[release("0.3.0", Some(1), true)],
                true,
                &id("0.3.0-beta.10", Some(800)),
                false
            )
            .status,
            "newer"
        );
        assert_eq!(
            select(
                &[release("0.4.0", Some(1), true)],
                true,
                &id("0.3.0", Some(800)),
                false
            )
            .status,
            "newer"
        );
        assert_eq!(
            select(
                &[release("0.3.0-beta.7", Some(2), true)],
                true,
                &id("0.3.0-beta.7", Some(1)),
                false
            )
            .status,
            "newer"
        )
    }
    #[test]
    fn uncertain_metadata_never_claims_current() {
        let current = id("0.3.0-beta.7", Some(1));
        assert_eq!(
            select(
                &[release(&current.version, None, true)],
                true,
                &current,
                false
            )
            .status,
            "unknown"
        );
        assert_eq!(
            select(
                &[release(&current.version, Some(1), true)],
                false,
                &current,
                false
            )
            .status,
            "unknown"
        );
        let mut dev = current.clone();
        dev.development = true;
        assert_eq!(
            select(
                &[release(&current.version, Some(1), true)],
                true,
                &dev,
                false
            )
            .status,
            "unknown"
        );
        let mut bad = release(&current.version, None, true);
        bad.body = Some(
            "<!-- sarge-release: {\"schema\":1,\"version\":\"other\",\"build\":20} -->".into(),
        );
        assert_eq!(select(&[bad], true, &current, false).status, "unknown");
        assert_eq!(
            select(
                &[release(&current.version, Some(1), true)],
                true,
                &id(&current.version, None),
                false
            )
            .status,
            "unknown"
        );
        assert_eq!(
            version("0.3.0-beta.7+git99")
                .unwrap()
                .cmp_precedence(&version("0.3.0-beta.7+git1").unwrap()),
            Ordering::Equal
        )
    }
    #[test]
    fn trust_artifacts_and_unrelated_releases() {
        let current = id("0.3.0-beta.7", Some(1));
        let mut wrong = release("0.3.0-beta.8", Some(1), true);
        wrong.assets[0].browser_download_url = "https://evil.example/installer.exe".into();
        assert_eq!(
            select(&[wrong], true, &current, false).status,
            "source_only"
        );
        let mut wrong = release("0.3.0-beta.8", Some(1), true);
        wrong.assets[0].name = "SARGE.Desktop_0.3.0-beta.8_arm64-setup.exe".into();
        assert_eq!(
            select(&[wrong], true, &current, false).status,
            "source_only"
        );
        for u in [
            "http://github.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.8",
            "https://github.com.evil.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.8",
            "https://github.com/other/repo/releases/tag/v0.3.0-beta.8",
            "https://user@github.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.8",
            "https://github.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.8?x=1",
        ] {
            assert!(!validate_release_url(u))
        }
        let mut draft = release("0.3.0-beta.8", Some(1), true);
        draft.draft = true;
        assert_eq!(select(&[draft], true, &current, false).status, "unknown");
        let mut other = release("0.3.0-beta.8", Some(1), true);
        other.html_url = "https://github.com/other/repo/releases/tag/v0.3.0-beta.8".into();
        assert_eq!(select(&[other], true, &current, false).status, "unknown");
        let mut mismatch = release("0.3.0-beta.8", Some(1), true);
        mismatch.prerelease = false;
        assert_eq!(select(&[mismatch], true, &current, false).status, "unknown");
        assert_eq!(
            select(
                &[
                    release("0.3.0-beta.10", None, false),
                    release("0.3.0-beta.8", None, true)
                ],
                true,
                &current,
                false
            )
            .release
            .unwrap()
            .version,
            "0.3.0-beta.8"
        )
    }
    #[test]
    fn disabled_throttle_dismissal_manual_retry() {
        let c = db();
        let current = id("0.3.0-beta.7", Some(1));
        let mut t = mock(vec![]);
        assert!(enabled(&c));
        set_automatic(&c, false).unwrap();
        assert_eq!(
            check_with(&c, false, 100, &current, &mut t).unwrap().status,
            "skipped"
        );
        assert_eq!(t.calls, 0);
        set_automatic(&c, true).unwrap();
        assert_eq!(
            check_with(&c, false, 100, &current, &mut t).unwrap().status,
            "failure"
        );
        assert_eq!(
            check_with(&c, false, 200, &current, &mut t).unwrap().status,
            "skipped"
        );
        assert_eq!(t.calls, 1);
        t.replies.push(reply(
            200,
            &serde_json::to_string(&vec![release("0.3.0-beta.8", None, true)]).unwrap(),
        ));
        let result = check_with(&c, true, 201, &current, &mut t).unwrap();
        assert_eq!(result.status, "newer");
        dismiss(&c, &result.release.unwrap().key).unwrap();
        dismiss(&c, "v0.3.0-beta.9:unknown").unwrap();
        t.replies.push(reply(304, ""));
        assert_eq!(
            check_with(&c, false, DAY + 202, &current, &mut t)
                .unwrap()
                .status,
            "skipped"
        );
        assert_eq!(
            check_with(&c, true, DAY + 203, &current, &mut t)
                .unwrap()
                .status,
            "newer"
        );
        assert_eq!(t.calls, 3)
    }
    #[test]
    fn malformed_rate_limit_cache_and_backoff() {
        let c = db();
        let current = id("0.3.0-beta.7", Some(1));
        let mut t = mock(vec![reply(200, "not json")]);
        assert_eq!(
            check_with(&c, true, 100, &current, &mut t).unwrap().status,
            "failure"
        );
        let mut limit = reply(429, "");
        limit.retry_after = Some("120".into());
        limit.remaining = Some("0".into());
        limit.reset = Some("300".into());
        t.replies.push(limit);
        assert_eq!(
            check_with(&c, true, 101, &current, &mut t).unwrap().status,
            "failure"
        );
        assert_eq!(load(&c).backoff_until, 300);
        assert_eq!(
            check_with(&c, true, 299, &current, &mut t).unwrap().status,
            "failure"
        );
        assert_eq!(t.calls, 2);
        t.replies.push(reply(200, "[]"));
        assert_eq!(
            check_with(&c, true, 301, &current, &mut t).unwrap().status,
            "unknown"
        );
        t.replies.push(reply(304, ""));
        assert_eq!(
            check_with(&c, true, 400, &current, &mut t).unwrap().status,
            "unknown"
        );
        assert_eq!(t.etags.last().unwrap().as_deref(), Some("\"fixture\""));
        assert_eq!(backoff(&reply(403, ""), 100), 160);
        assert!(
            backoff(
                &Reply {
                    retry_after: Some("Mon, 26 Oct 2026 15:00:00 GMT".into()),
                    ..reply(429, "")
                },
                100
            ) > 1000
        )
    }
    #[test]
    fn missing_cache_redirect_and_pagination_bound() {
        let c = db();
        let current = id("0.3.0-beta.7", Some(1));
        for s in [304, 302, 500] {
            let mut t = mock(vec![reply(s, "")]);
            assert_eq!(
                check_with(&c, true, 100, &current, &mut t).unwrap().status,
                "failure"
            )
        }
        let mut unrelated = release("0.3.0", None, false);
        unrelated.tag_name = "not-a-version".into();
        let body = serde_json::to_string(&vec![unrelated; 100]).unwrap();
        let mut t = mock((0..MAX_PAGES).map(|_| reply(200, &body)).collect());
        assert_eq!(
            check_with(&c, true, 200, &current, &mut t).unwrap().status,
            "unknown"
        );
        assert_eq!(t.calls, MAX_PAGES);
        assert!(!load(&c).complete)
    }
    #[test]
    fn exhausted_allowance_on_success_stops_pagination() {
        let mut r = reply(200, "[]");
        r.remaining = Some("0".into());
        r.reset = Some("500".into());
        let mut t = mock(vec![r]);
        let cache =
            fetch(&mut t, &Cache::default(), 100).unwrap_or_else(|_| panic!("valid final page"));
        assert_eq!(cache.backoff_until, 500);
        let body = serde_json::to_string(&vec![release("0.3.0-beta.7", None, true); 100]).unwrap();
        let mut r = reply(200, &body);
        r.remaining = Some("0".into());
        r.reset = Some("500".into());
        let mut t = mock(vec![r]);
        assert_eq!(fetch(&mut t, &Cache::default(), 100).err().unwrap().1, 500);
        assert_eq!(t.calls, 1);
    }
    #[test]
    fn malformed_metadata_duplicate_builds_and_missing_artifacts() {
        let current = id("0.3.0-beta.7", Some(1));
        for body in [
            "<!-- sarge-release: broken -->",
            "<!-- sarge-release: {\"schema\":1,\"version\":\"0.3.0-beta.7\",\"build\":0} -->",
            "<!-- sarge-release: {} --> <!-- sarge-release: {} -->",
        ] {
            let mut r = release(&current.version, Some(1), true);
            r.body = Some(body.into());
            assert_eq!(select(&[r], true, &current, false).status, "unknown");
        }
        assert_eq!(
            select(
                &[
                    release(&current.version, Some(1), true),
                    release(&current.version, None, true)
                ],
                true,
                &current,
                false
            )
            .status,
            "unknown"
        );
        let mut pending = release("0.3.0-beta.8", Some(1), true);
        pending.assets[0].state = "new".into();
        assert_eq!(
            select(&[pending], true, &current, false).status,
            "source_only"
        );
        let mut empty = release("0.3.0-beta.8", Some(1), true);
        empty.assets[0].size = 0;
        assert_eq!(
            select(&[empty], true, &current, false).status,
            "source_only"
        );
        let mut other_platform = current.clone();
        other_platform.platform = "macos_aarch64".into();
        assert_eq!(
            select(
                &[release("0.3.0-beta.8", Some(1), true)],
                true,
                &other_platform,
                true
            )
            .status,
            "source_only"
        );
    }
}
