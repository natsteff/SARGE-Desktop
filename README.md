# SARGE Desktop · Beta

<img src="docs/images/sarge-logo.png" alt="SARGE — Scole’s Archive of RAWs, Galleries & Exports. No RAW left behind." width="360">

**Scole’s Archive of RAWs, Galleries & Exports**  
*Reporting for photo duty*

[**Download Windows Beta 0.3.0-beta.2**](https://github.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.2) · [All releases](https://github.com/natsteff/SARGE-Desktop/releases) · [Setup and usage](desktop/README.md)

SARGE’s main purpose is to **inventory photo images across multiple drives**, including USB and external drives that are currently disconnected. It focuses on **matching Canon RAW (CR2) originals to JPG exports**, keeping detailed per-image records and aggregate counts and sizes across your collection. Copy matched RAWs and exports—or unmatched files—to selected destinations to support storage and space-saving management. Copies are verified; SARGE does not delete your originals or reclaim space automatically.

Windows 10/11 x64 is the primary target. No Docker, Python, helper, or ExifTool installation is required. macOS is a bonus development target. This beta installer is unsigned and needs further hands-on testing.

## Features

- Register multiple drives or folders with nicknames, descriptions, volume identity and dates extracted from folder names.
- Scan nested folders without a depth limit; directory links are not followed.
- Match RAW identifiers in export filenames and parent folders; review suggestions and confirm relationships manually.
- Sort every optional metadata column, customize actual and mapped EXIF columns, and export CSV. **EXIF Filters** provides searchable catalog-derived choices, missing values and numeric/date ranges, including offline sources.
- Read shutter speeds in conventional form, such as **0.6 s** or **1/250 s**, while keeping numeric precision for sorting and filtering.
- Build filename mappings visually in **Filename conventions**, using an example file, highlighted text, field picklists and file-type checkboxes. The default CR2 rule maps the prefix before `_` to Camera body (Mapped), separately from actual EXIF.
- Keep metadata and optional previews in the local SQLite catalog for offline browsing.
- Open original photos or reveal them in Explorer when their source is available.
- Run Scan only, Scan and copy, or Copy only on an individual source or all available registered sources. Confirm before skipping offline sources.
- Copy matched, unmatched or both categories, preserving folders, verifying SHA-256 and protecting existing different files.
- View prominent online/offline counts and per-location file/RAW/export/matched/unmatched counts and sizes, including offline catalogs.
- Monitor bulk scan/copy operations as one activity block with aggregate progress and expandable location results.
- Back up the catalog, cached previews and preferences in one archive with restore instructions.

## Appearance and screenshots

Use **Appearance** in the sidebar to switch between **SARGE · Default**, with light backgrounds and olive accents, and **Solly’s Soldiers · Dark**, with white text and red, blue and green accents. The logo changes with the theme. Your choice is remembered on this computer.

These captures show the installed Windows beta 0.3.0-beta.1 with a small test photo catalog. Updated captures of the beta.2 summaries, EXIF filters and filename configuration are pending; the images below have not been relabeled as the new version. See the [capture checklist](docs/SCREENSHOTS.md).

Photo inventory in the default theme, with cached previews, RAW/export match status and saved camera metadata.

![SARGE Desktop Photo inventory in the default theme, showing photo previews, matched RAWs and exports, and saved EXIF columns](docs/images/inventory-default.png)

Drives & locations shows registered sources, online/offline counts and controls for scanning, copying and backing up the catalog.

![SARGE Desktop Drives and locations in the default theme, showing three registered locations, two online and one offline](docs/images/drives-default.png)

Scan activity records completed jobs and their copy, skip and intentionally omitted counts.

![SARGE Desktop Scan activity in the default theme, showing completed copy jobs and per-job results](docs/images/activity-default.png)

The same inventory in Solly’s Soldiers · Dark, with its alternate logo and the Appearance switcher visible.

![SARGE Desktop Photo inventory in Solly’s Soldiers dark theme, showing the alternate mascot logo and Appearance selector](docs/images/theme-soldiers.png)

## Updating from the desktop alpha

Fully close SARGE and back up your catalog before installing. The database location is unchanged:

```text
%APPDATA%\io.github.natsteff.sarge.desktop
```

The app is now named **SARGE Desktop** rather than **SARGE Desktop Alpha**. If Windows retains the old Alpha installation, uninstall that entry with **Delete app data unchecked**, then use SARGE Desktop. Both versions use the same catalog; do not run them simultaneously. Your original photos are separate from the catalog and remain untouched.

Existing inventories need a rescan to collect the newest metadata fields. Preview generation is optional and can be enabled in Preview settings.

## Development

```sh
git clone https://github.com/natsteff/SARGE-Desktop.git
cd SARGE-Desktop/desktop
pnpm install --frozen-lockfile
pnpm dev
```

Developers need Rust stable, Node.js 22+, pnpm and the platform build tools. GitHub Actions tests Windows/macOS and builds the Windows installer. Development can continue on either platform. [Detailed instructions](desktop/README.md).

This independent repository starts from the desktop implementation developed in the earlier SARGE project, through commit `ecca779`. The Docker application is preserved in that separate repository and is no longer the focus of development. No Docker services, scanner helpers or inventory import scripts are included here.

## Development disclaimer

SARGE Desktop was developed using **OpenAI Codex**, with human direction and review. AI-assisted development and automated tests do not guarantee error-free behavior. This beta still needs hands-on validation: test scan and copy workflows on a small collection with backups before using them for storage management.

AGPL-3.0-only. See [LICENSE](LICENSE).
