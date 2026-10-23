# Windows screenshot checklist

Capture the installed SARGE Desktop **0.3.0-beta.5 or newer** on the Windows PC using the current catalog. Current README images were captured on beta.2; replace them with actual new captures when the UI or version changes. No Docker screens, generated screens or fabricated inventory.

Use **SARGE · Default** for all standard screens. Use one consistent window size (at least 1380 × 900; wider if needed for columns), keep the app title/version visible where possible and close unrelated dialogs and transient notices. Review the real names, paths and image previews for suitability before publishing.

## Required captures

| PNG under docs/images | What to show |
| --- | --- |
| inventory-default.png | Media Inventory with previews, representative EXIF columns and Camera body (Mapped) immediately beside Camera model (EXIF). Show a readable shutter value such as 0.6 s or 1/60 s. Keep the column headers readable and avoid an excessively wide selection. |
| drives-default.png | Drives & locations with prominent registered/online/offline counts and at least one location card showing All files, RAW originals, Edited/exported, Matched RAWs and Unmatched RAWs and Videos, each with count and size. Include an offline location if practical. |
| activity-default.png | A NEW Scan All, Copy All or bulk Scan and copy operation shown as one block with combined counts/status. Expand Results by location for one operation. Older beta.1 history is intentionally ungrouped. |
| exif-filters-default.png | The EXIF Filters dialog with a representative field selected, available-value counts and numeric/date ranges where appropriate. Use saved catalog data. An optional extra capture can show applied filter chips above inventory. |
| filename-conventions-default.png | Sidebar Filename conventions settings with the enabled default CR2 prefix-to-Camera body mapping and rule controls visible. |
| filename-rule-builder-default.png | Visual rule builder: an example such as 1DX_1234.cr2, highlighted 1DX, Camera body (Mapped), CR2 file-type selection, and tested filename results. No typed template syntax is required. |
| theme-soldiers.png | One Media Inventory example in Solly’s Soldiers · Dark, with its alternate logo and Appearance switcher visible. Use the same representative inventory as the default example. |

## Beta 4 additions and verification

- **inventory-default.png:** include both images and a video, all five top totals, and the All media filter. Show the wider filename/location column with a long filename. Keep optional metadata columns readable; horizontal scrolling is available.
- **inventory-scrolled-default.png (additional):** scroll inside the inventory table to demonstrate that column headings remain visible above lower rows.
- **drives-default.png:** show the new **Open in Explorer** action on an online location, with an offline location showing the action disabled if practical. Check sidebar settings-button gaps and spacing above location cards.
- **activity-default.png:** complete a new bulk operation before capture. Confirm the “Job in progress” notice clears and no stale “See Scan activity” instruction remains on this page.
- **formats-matching-default.png (additional):** Tools → Formats & matching, showing supported image/video inventory formats and default CR2 → JPG/JPEG/PNG matching rules.
- **copy-videos-default.png (additional, optional):** Tools → Copy all videos review, or a copy dialog showing the option to include videos in the unmatched destination. Avoid starting unwanted copy operations just for screenshots.
- Capture all standard examples in the default theme; retain only **theme-soldiers.png** as the dark-theme example.

## Capture and handoff

1. Install/open the current beta and confirm the version. Rescan sample locations if necessary to populate native EXIF numbers. Mapped filename fields update automatically from saved filenames when rules are saved.
2. Enable previews if desired, choose suitable columns, and run a new bulk job before capturing activity.
3. Use Windows Snipping Tool (or an available, authorized Windows capture tool) to capture the actual app window. Save PNGs with the names above; do not alter values or fabricate file rows.
4. Return Appearance to the default theme afterward.
5. Provide the PNGs in a ZIP to the coordinating chat. Include optional README edits as reference material. The user plans to recapture on Windows and provide the results for publishing.
6. Update README.md captions/alt text for the actual captured version. Keep standard examples in the default theme and only one dark-theme example. Include the new filename settings/builder and EXIF filter screens.

The full README SARGE logo is docs/images/sarge-logo.png. Keep it separate from the white-background app/favicon asset.

## Beta 5 settings captures to refresh

- **formats-matching-default.png:** show the upfront Configured matching pairs list with CR2 and CR3 both configured. Open the RAW format selector for an additional **formats-matching-dropdown-default.png** capture showing configured formats labeled and sorted first. Changes are drafts until saved; use intended settings or restore them afterward.
- **filename-conventions-default.png:** show the disabled Add default CR2 filename rule button while the default rule exists, its Remove and enable/disable controls, and the explanation distinguishing filename extraction from format matching.
- Retain the beta 4 capture checks above. Existing README images remain labeled beta 2 until new Windows captures are supplied.
