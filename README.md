# SARGE Desktop · Beta

<img src="docs/images/sarge-logo.png" alt="SARGE — Scole’s Archive of RAWs, Galleries & Exports. No RAW left behind." width="360">

**Scole’s Archive of RAWs, Galleries & Exports**  
*Reporting for photo duty*

[**Download Windows Beta 0.3.0-beta.8**](https://github.com/natsteff/SARGE-Desktop/releases/tag/v0.3.0-beta.8) — Adds manual and optional startup update checks, consistent version/build information, and official release links for manual installation.

[All releases](https://github.com/natsteff/SARGE-Desktop/releases) · [Setup and usage](desktop/README.md)

SARGE’s main purpose is to **inventory photos and videos across multiple drives**, including USB and external drives that are currently disconnected. It tracks individual media files along with counts and storage sizes across your collection, even when a registered drive is offline. For photos, SARGE can **match RAW originals with their edited/exported images** using configurable file-format pairs; **Canon CR2 to JPG is just a default example**, not a limitation. Multiple RAW and export formats can be configured to suit different cameras and workflows. Copy matched RAWs and exports—or unmatched files—to selected destinations to support storage management. Copies are verified; SARGE does not delete your originals or reclaim space automatically.

Windows 10/11 x64 is the primary target. No Docker, Python, helper, or ExifTool installation is required. macOS is a bonus development target. This beta installer is unsigned and needs further hands-on testing.

## Windows security warnings

SARGE’s current beta releases are **unsigned**. Depending on your Windows settings and Microsoft’s reputation checks, you may see a security warning. Some installations won’t show one.

Download installers only from the [official SARGE Desktop releases](https://github.com/natsteff/SARGE-Desktop/releases).

- **“Windows protected your PC” — Microsoft Defender SmartScreen:** select **More info → Run anyway**, if that option is available and you trust the downloaded installer.
- **“Smart App Control blocked this app”:** Smart App Control has no per-app exception. You can wait for a signed release or choose to turn it off under **Windows Security → App & browser control → Smart App Control settings**. Turning it off affects protection for your entire PC; the ability to turn it back on depends on your Windows version and updates. See [Microsoft’s Smart App Control guidance](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions) before changing this setting.

If **Microsoft Defender Antivirus reports a named threat**, don’t assume it is the same warning. Stop and [report the detection](https://github.com/natsteff/SARGE-Desktop/issues), including the release version and detection name.

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

Beta.8 adds manual **Help → Check for Updates…** and a saved, default-enabled startup check at most once per 24 hours. It contacts GitHub without sending catalog contents, filenames or paths, and can be disabled in Settings. Installation remains manual. See [update-check behavior and review plan](docs/UPDATE-CHECKS.md).

## Appearance and screenshots

Open **Settings → Appearance** using the sidebar gear or **SARGE Desktop → Settings…** in the application menu to switch between **SARGE · Default**, with light backgrounds and olive accents, and **Solly’s Soldiers · Dark**, with white text and red, blue and green accents. The logo changes with the theme. Your choice is remembered on this computer.

These screenshots show the installed Windows **0.3.0-beta.6**, captured on October 24, 2026 with a sample media catalog. Standard examples use the default theme; the final example shows Solly’s Soldiers · Dark. Click a screenshot to inspect its full resolution.

Media Inventory keeps previews, RAW/export matches, sortable EXIF columns and filename-derived values together. **Camera model (EXIF)** and **Camera body (Mapped)** are separate: the example shows Canon EOS 6D alongside the filename label `1DX`. Shutter speeds appear as **0.6 s** and **1/60 s**.

![SARGE Desktop beta 6 Media Inventory with five media totals, sample previews, separate EXIF and mapped camera columns, and readable shutter speeds](docs/images/inventory-default-beta6-cropped.png)

The sidebar gear and **SARGE Desktop → Settings…** open one Settings screen. Its five sections cover appearance, formats and matching, filename conventions, photo previews, and catalog tools. Appearance changes save immediately; processing preferences use each section’s Save button.

![SARGE Desktop beta 6 consolidated Settings screen showing the single sidebar gear, five settings sections and Appearance theme selector](docs/images/settings-appearance-default.png)

**Settings → Formats & matching** lists configured RAW/export pairs. Multiple RAW formats can be active together; this example has CR2 and CR3 paired with JPG/JPEG/PNG. Select a format to edit its targets, then save to apply the changes.

![SARGE Desktop beta 6 Formats and matching settings with CR2 and CR3 configured and JPG, JPEG and PNG targets selected](docs/images/formats-matching-default.png)

Drives & locations shows registered, online and offline totals. Each location includes file counts and sizes for all files, RAW originals, edited/exported images, matched and unmatched RAWs, and videos. **Open in Explorer** reveals an available source.

![SARGE Desktop beta 6 Drives and locations showing three registered sources, two online, one offline, category counts and sizes, and an enabled Open in Explorer action](docs/images/drives-default.png)

Bulk operations appear as one activity block with combined results. Expand **Results by location** to inspect each source. The captured sample scan completed both online locations; it reports 12 indexed files, 10 previews and one unavailable item, with no copies performed.

![SARGE Desktop beta 6 Scan activity with a completed two-location bulk scan and expanded per-location results](docs/images/activity-default.png)

**EXIF Filters** provides searchable saved values and counts, plus numeric or date ranges where applicable. Filters work independently of displayed columns and combine with source, file-type, match-state and search controls.

![SARGE Desktop beta 6 EXIF and mapped filters dialog with ISO values, file counts and minimum and maximum range controls](docs/images/exif-filters-default.png)

**Settings → Filename conventions** manages mapped fields without changing embedded EXIF or original files. The enabled default extracts the prefix before `_` as **Camera body (Mapped)** for CR2 files. Rules can be edited, disabled, removed and reordered; saving reapplies them to saved filenames, including offline locations.

![SARGE Desktop beta 6 Filename conventions settings with the enabled default CR2 rule and disabled duplicate-default button](docs/images/filename-conventions-default.png)

Build a rule visually: choose an example, highlight the desired value, select its mapped field and applicable file types, then test against saved filenames. This capture shows `1DX` highlighted in `1DX_1234.cr2`, with Camera body (Mapped) and CR2 selected. Test results appear farther down the dialog.

![SARGE Desktop beta 6 visual filename rule builder with 1DX highlighted, Camera body mapped field selected and CR2 enabled](docs/images/filename-rule-builder-default.png)

Media Inventory in **Solly’s Soldiers · Dark**, with its alternate logo and the single settings gear. The table is scrolled to lower sample rows, including a video whose photo-match status is Not applicable.

![SARGE Desktop beta 6 Media Inventory in Solly’s Soldiers dark theme with image previews, a video row, five totals and the sidebar settings gear](docs/images/theme-soldiers.png)

## Development disclaimer

SARGE Desktop was developed using **OpenAI Codex**, with human direction and review. AI-assisted development and automated tests do not guarantee error-free behavior. Scan and copy workflows have been tested on small collections. We are now seeking results and feedback from larger-scale use. Keep backups when using SARGE for storage management.

AGPL-3.0-only. See [LICENSE](LICENSE).
