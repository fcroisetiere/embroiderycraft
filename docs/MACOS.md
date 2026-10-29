# EmbroideryCraft on macOS

Works on macOS 11 (Big Sur) and later, on Apple Silicon (M1 and later) and Intel Macs.

## Installing
1. Unzip `EmbroideryCraft-…-macos.zip` and drag **EmbroideryCraft.app** into **Applications**.
2. The first time, open it with a **right-click (or Control-click) › Open**, then **Open**.
   The app is not signed with an Apple Developer ID, so a plain double-click is refused once.
   On macOS 15 and later, if there is no Open button: try to open it once, then go to
   **System Settings › Privacy & Security**, scroll down and click **Open Anyway**.
   After that it opens normally.
   (If you built the app on this Mac with `tools/build-macos.sh`, none of this is needed.)

`embcraft` next to the app is the command-line tool (DST info, previews, the trim test pattern).

## Using it on a Mac
- Shortcuts use ⌘: ⌘S save, ⌘O open, ⌘E export DST, ⌘Z / ⇧⌘Z undo / redo, ⌘D duplicate, ⌘P stitch player.
- Trackpad: **pinch** to zoom, **two fingers** to move around; ⌘ + scroll also zooms.
  Hold **Space** and drag, or drag with a right-click, to pan.
- Japanese and Korean: **Hiragino Sans** and **Apple SD Gothic Neo** come with macOS and appear
  in the lettering window; Noto Sans CJK (free) can be added with Font Book. Convert a font once
  with **Fonts › Make an EmbroideryCraft font…** for the best satin.

## Where things are kept
- **EmbroideryCraft fonts** (`.ecfont`): `~/Library/Application Support/EmbroideryCraft/fonts`
  (in Finder: **Go › Go to Folder…** and paste it). The **Fonts** menu shows the exact folder.
- **Projects** (`.embcraft`) and **DST files**: wherever you save them.
- **Background pictures** are linked, not copied: keep the picture next to the project when moving it.

## Building it
On a Mac, from the project folder:
```
tools/build-macos.sh
```
It installs what it needs (Xcode command line tools, Rust) and writes
`dist/EmbroideryCraft-<version>-macos.zip` with one app for both Apple Silicon and Intel.

Without a Mac: `.github/workflows/build.yml` builds the macOS and Windows zips on GitHub's
machines whenever the project is pushed to a GitHub repository (Actions tab › latest run ›
Artifacts).

Signing with an Apple Developer ID and notarizing (Apple Developer Program, USD 99 a year)
would remove the first-open step for everyone.
