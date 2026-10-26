# Update checks — beta.8

Nate authorized publishing this implementation as a Windows test release through the primary GitHub repository on October 26, 2026. The authoritative release identity is **0.3.0-beta.8 (Build 1)**. Older installers are unchanged. Windows acceptance is performed on the PC after installing the release; no Mac development review is required.

## Behavior

Help → Check for Updates… shows installed identity and a newer/current/unknown/failure result. A source-only release is identified separately from a suitable platform installer. View Release opens only an official `https://github.com/natsteff/SARGE-Desktop/releases/tag/...` page. SARGE never downloads an installer, installs, restarts or modifies its own binary.

Settings → Updates contains “Automatically check for updates at startup”, enabled by default and saved immediately. A nonblocking startup check runs at most once per 24 hours, including failed attempts. Disabling it prevents automatic requests; manual checks remain available. Only a newer eligible installer causes a nonmodal startup notice. Later remembers the dismissed release; manual checks can still show it. Automatic/offline errors remain quiet. Closing the separate update dialog leaves catalog jobs and other dialogs intact; any bounded request already in flight may finish in the background.

Requests go only to GitHub’s public HTTPS release-list API, with no authentication token, catalog content, filenames or paths. GitHub receives ordinary connection information, including the IP address. Requests are serial, limited to three pages/300 releases, one MiB per page and a 15-second overall network budget with a five-second connection limit. Redirects and automatic retries are disabled. Saved ETags enable conditional requests, successful manual results can be reused for 60 seconds, and 403/429 or exhausted allowances respect Retry-After/reset cooldowns. A truncated listing cannot prove the app current. Cache, startup timing, preference and dismissal keys live in the existing SQLite settings table and are included in catalog backups.

Stable installs consider stable releases. Alpha/beta/rc installs also consider eligible prereleases and stable successors. Drafts, unsupported tags, malformed version sequences, mismatched channel flags, foreign URLs and wrong/unuploaded/empty installer assets are excluded. Numeric prerelease sequences use SemVer precedence, so beta.7 < beta.8 < beta.10 < stable. Versions compare before build numbers; SemVer +metadata does not order builds. Historical releases with missing metadata cannot establish same-version build equality.

## Authoritative identity and future release preparation

`desktop/release.json` is authoritative for version and the positive per-version build number. Existing public tags stay unchanged. `pnpm version:sync` synchronizes package.json, Cargo.toml/Cargo.lock, tauri.conf.json and generated ui/build-info.js. `pnpm version:check` and Rust build.rs reject drift. About, footer, Settings and update results use the Rust runtime identity; source revision/modified state are build-time diagnostics, never version-ordering evidence. Debug/modified builds cannot claim equality to a production release.

The beta.8 identity is **0.3.0-beta.8 (Build 1)**. Debug or modified local builds identify themselves as development builds. Release CI builds from a clean checkout; the release marker below enables a definite same-version/build comparison. The separate build number does not rename historical releases or alter the existing SemVer/Tauri OS bundle mapping.

For each authorized release, append the exact single machine-readable block printed by `node scripts/version.mjs --release-note` to its GitHub release body. Format:

```text
<!-- sarge-release: {"schema":1,"version":"0.3.0-beta.8","build":1} -->
```

The checker requires the metadata version to match the tag and treats missing, malformed, duplicate or zero build metadata as unknown for same-version comparisons. Never retrofit certainty by editing old releases. Public artifacts should retain unique version tags and GitHub’s immutable artifact digest; do not silently replace installers.

## Local review

With Node/pnpm and Rust available:

```sh
cd desktop
pnpm test
cd src-tauri
cargo test --locked --lib
cargo build --locked
```

To review interactively, close any running SARGE instance, return to `desktop` and run `pnpm dev`. On this Mac, the isolated Rust toolchain and bundled Node can be used with `sh /tmp/sarge-review-updates.sh` (temporary local review launcher; not a distributed artifact). Check Settings → Updates, the native Help menu and Source/About. This opens the actual local app/catalog and may perform a startup GitHub check if enabled; use an isolated app-data environment for test fixtures. No real catalog was launched during implementation verification. The headless/UI tests use mocks; Rust fixtures use in-memory settings and no network.

## Targeted Windows acceptance for a later approved test build

1. Verify the exact new version/build and source diagnostics in About, footer and update results; do not confuse this implementation with the currently published beta.7.
2. Confirm startup stays responsive and navigation/jobs remain usable. With a controlled newer-release fixture, show one nonblocking notice, choose Later, restart and confirm suppression. Do not manufacture a public release just for this check.
3. Disable automatic checks, restart and verify no automatic update request. Manual Help checking must still work. Enable again and verify startup throttling across restarts.
4. Confirm manual newer/current/unknown/source-only/error states with controlled fixtures, including missing historical build metadata, beta.7/8/10, stable promotion, version/build reset, wrong/missing installers and malformed responses. Test rate-limit cooldown, offline timeout and a manual retry. Live same-version historical beta.7 is expected to be unknown, not falsely current.
5. Close/cancel an update check while browsing or an authorized sample job is active. File operations must not be cancelled or changed. View Release must open only the official release page, with no automatic installer transfer/install/restart.
6. Check default/dark themes and a 900 × 650 window. Existing captured beta.6 labels remain unchanged. No new screenshot batch or Windows operations should start solely from this handoff.

Record actual Pass/Fail/Blocked/Not run and exact build for each check. Windows execution, live offline/rate-limit behavior and installed-app persistence are not established by mocked tests or macOS compilation.

## Implementation verification record — October 26, 2026

- **38 JavaScript tests passed**, including existing scan/copy UI cases, update preference/notification/dialog behavior and identity drift checks.
- **33 Rust tests passed** with `cargo test --locked --lib`, including existing catalog/copy safety cases and mocked release selection, numeric prereleases, stable promotion/build resets, missing/malformed metadata, trusted URLs/artifacts, disabled/throttled startup checks, multiple dismissals, manual retry, malformed/offline/rate-limited responses, ETags and bounded pagination.
- **macOS debug app compilation passed** with `cargo build --locked`; no packaged Windows build or live app/copy test was run.
- Browser layout checks used a mocked catalog and mocked release response in default/dark at 1380 × 900 and 900 × 650. Settings, manual/unknown results, About and closing controls fit without browser errors. No screenshots or real update requests were generated. A real-script-load-order issue found in this check was fixed and covered by a regression test.
- Version consistency and whitespace checks passed. Initial implementation was local on base `d592b29`, before Nate authorized beta.8 publication. No new scheduler or screenshot refresh was introduced. The release pipeline must pass its Windows/macOS gates for the versioned beta.8 build before publication.
