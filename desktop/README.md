# SARGE Desktop — Windows Beta 0.3.0-beta.1

Windows-focused native desktop edition. macOS remains a bonus development target. The earlier Docker project is retained separately and is no longer the development focus.

## Windows test build

Prefer the installer on the [Windows desktop beta release](https://github.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.1), once available. For development builds, download the **SARGE-Windows-x64-Beta** artifact from the latest successful **Build SARGE Desktop** GitHub Actions run on this branch. Extract the artifact ZIP and run the `SARGE Desktop_*_x64-setup.exe` installer. GitHub Actions artifact downloads require signing into GitHub. An unsigned prerelease installer may also be attached to GitHub Releases.

This beta targets Windows 10/11 x64 and installs for the current user. No Docker, Python, Tcl/Tk, PowerShell scanner, or ExifTool installation is required. The installer uses Microsoft's WebView2 bootstrapper if the runtime is absent; that step may require an internet connection. The app works offline afterward. Installer downloads are unsigned during this beta, so Windows may warn or apply organizational restrictions.

## First scan and copy

1. Open **SARGE Desktop**. Choose **Add drive / folder**, then use the native picker for a drive root or any accessible folder. **Drives & locations → Choose attached drive** lists connected volumes.
2. Review source nickname, physical drive name, description, and folder date. Volume label, ID, and letter are detected on Windows. Save the source.
3. Choose **Scan / copy** on its card:
   - **Scan only**: index CR2 originals and JPG/JPEG/PNG exports recursively.
   - **Scan and copy**: scan, match, then immediately copy within this job.
   - **Copy only**: copy using the last completed inventory of this source. A failed/partial/cancelled new scan invalidates copy eligibility until a complete scan succeeds.
4. For either copy mode, pick either or both existing **matched** and **unmatched** destination folders outside every selected source. At least one is required. When one is empty, confirm that its category will be intentionally omitted. The chosen folders must not overlap each other or any selected source. **Copy All Attached Drives** offers Copy only or Scan and copy across registered available sources; offline and removed sources are excluded. Each source gets its own name-and-ID subfolder under each chosen destination. In a bulk scan-and-copy batch all scans finish before copying starts. Renaming a source changes its destination subfolder name for subsequent bulk jobs.
5. Check **Scan activity** for counts, cancellation, completed copies, identical files, skips, and errors.

Copies preserve the selected source's relative directory hierarchy. Matched RAWs and their confirmed exports go to the matched destination. Files without relationships go to the unmatched destination. Ambiguous and suggested relationships are skipped for review. Confirmed relationships spanning selected sources are supported in bulk copying; relationships to unselected sources are skipped. Intentionally omitted category counts are separate from errors/skips. Copies check source size/mtime against the scan, verify SHA-256, and never overwrite a different existing file. An identical destination counts as already present. Completed copies remain when a job is cancelled. Empty temporary files may remain after a process/OS crash; originals are never deleted.

Sources retain indexed details when disconnected. Reassociate a source after its drive letter/location changes; its catalog ID and manual match decisions remain. Windows volume identity checks prevent scanning a different disk that reuses the same letter. Same names/locations prompt **Update existing** or **Add as separate**. Separate overlapping registrations can count the same files twice. Remove is reversible; permanent deletion removes only catalog details after typing DELETE.

The photo inventory supports search, source/type/match filters, sortable name/date/size columns, manual relationship confirmation/rejection, CSV export, and decimal MB/GB statistics. The default and Solly's Soldiers themes include their corresponding logos and persist on this computer.

## Data and scope

Each computer has a separate local SQLite catalog. On Windows it is under the current user's application data directory for `io.github.natsteff.sarge.desktop` (shown at the bottom of the app). It does not open or modify the Docker database. This is a desktop app, not a LAN web server. No helper pairing or file import screens are included. Basic embedded EXIF uses a native Rust library; unsupported metadata is left empty without preventing file inventory. Folder dates and camera filename prefixes are still extracted independently. Catalog migration from Docker is not included. Desktop updates retain the same local catalog. Close the app before installing an update.

Test on a small copied photo set first. Suggested checks: names with extra descriptions, dated folders, multiple edits per RAW, ambiguous duplicates, unplug/replug and changed drive letter, all three modes, repeat copying, conflicting destinations, cancellation and both themes. Report the app version and job error text; no personal photos need to be uploaded.

## Develop on Mac or Windows

Build dependencies are for developers only: Rust stable, Node.js 22+, pnpm, and platform build tools (Visual Studio C++ build tools/WebView2 on Windows, Xcode command line tools on macOS).

```sh
git clone https://github.com/natsteff/SARGE-Desktop.git
cd SARGE-Desktop
git switch main
cd desktop
pnpm install --frozen-lockfile
pnpm dev
```

Run `cargo test --locked --lib` inside `desktop/src-tauri`. `pnpm build` creates the Windows installer on Windows. On macOS use `pnpm exec tauri build --bundles app` for the bonus native app; it is not signed/notarized for general Mac distribution. The GitHub workflow tests both OSes and builds the Windows installer on a Windows runner, so Mac development does not require cross-compiling Windows locally.

Core processing is shared Rust code across Windows/macOS. Filename/date matching rules mirror the Docker edition, with regression tests for boundaries, multiple exports, manual decisions, stale files, cancellation, copy collisions and path escapes. The desktop catalog schema is intentionally separate from the Python catalog; future behavior changes should update the relevant tests in both editions.

License: AGPL-3.0-only, see [LICENSE](../LICENSE).

## Optional photo previews

Open **Preview settings** under Appearance. Previews are off by default. Enable **Store and display photo previews**, then scan normally or choose **Generate missing previews** to populate the current inventory.

Small JPEG previews (up to 160 × 160 pixels) are stored in SQLite, so they remain visible when a drive is offline. JPG/JPEG and PNG files are decoded natively, with orientation applied where available. For CR2, SARGE tries embedded JPEGs in the first 32 MiB of the RAW; if none can be decoded it uses a confirmed matched export when available. Unsupported images retain a placeholder; a preview failure does not prevent inventory or copying. This is not full RAW demosaicing.

Click the preview/placeholder to **open the original** in the operating system's associated photo application. The original drive must be connected, the volume identity must match, and the file must still be available. A RAW preview derived from an export still opens the original RAW. Opening CR2 requires an associated application that supports it.

Turning previews off hides them and stops generation while retaining the cache. **Clear stored previews** removes only cached image data; later scans can regenerate it. Settings show the stored preview data size. Previews are loaded only for the visible inventory page.

## Catalog backup, offline sources, and photo details

**Drives & locations** shows registered location totals and online/offline counts. **Hide offline locations** hides only their cards; saved photos and statistics remain in the inventory. Scan All and Copy All ask before skipping offline locations. Removed sources are excluded. Disconnections during processing are reported in Scan activity.

**Back up catalog…** saves a `.tar` archive with a consistent SQLite snapshot, cached previews, preview settings, saved column choices, version/theme manifest and `RESTORE.txt`. Wait until jobs finish first. Original photographs and copied files are not included. Windows File Explorer can extract TAR archives; follow the included restore instructions with SARGE fully closed. Theme is recorded in the manifest and can be reselected in Appearance after restoring.

For a manual backup, fully close SARGE and copy the entire `%APPDATA%\io.github.natsteff.sarge.desktop` directory, including any SQLite `-wal`/`-shm` files. Do not copy a live database by itself. On macOS the catalog path appears at the bottom of the app. Updates retain this directory; if the installer offers to delete app data during uninstall, leave that option unchecked to keep scans.

Use **Customize columns** in Photo inventory for camera, lens, capture time, dimensions, ISO, aperture, shutter speed, focal length, filesystem creation and modification time. Available embedded metadata is collected and stored during scans, so details remain accessible offline. Existing catalogs need a rescan to collect the new fields. File details also show the complete saved metadata. Unsupported fields show a dash or Not available; full metadata support depends on the image format. RAW resolution is taken from EXIF pixel dimensions, never from its thumbnail. File creation time may change when copied and is separate from photo capture time and the date encoded in a folder name.

**Open file location** reveals and selects the original in Windows Explorer (Finder on Mac), after checking the registered volume and file. It is disabled for offline or missing files.

Existing previews regenerate only during scans when their source file or matched preview changes. **Generate missing previews** fills empty entries without replacing existing cached images. Unchanged polling retains thumbnail elements and does not repeatedly fetch their image data. Scans traverse nested folders without a depth limit; directory symlinks/junctions are not followed.
