# EmbroideryCraft on Windows

## Installing
1. Unzip `EmbroideryCraft-…-windows-x64.zip` anywhere, for example in `Documents\EmbroideryCraft`.
2. Double-click `EmbroideryCraft.exe`. Nothing else needs to be installed.
   - The first time, Windows SmartScreen may say the app is from an unknown publisher (it is not
     signed). Click **More info**, then **Run anyway**.
3. Optional: right-click `EmbroideryCraft.exe` › **Show more options** › **Send to › Desktop
   (create shortcut)**.

`embcraft.exe` is the command-line tool (DST info, previews, the trim test pattern); most people
only need `EmbroideryCraft.exe`.

## Where things are kept
- **EmbroideryCraft fonts** (`.ecfont`): `%APPDATA%\EmbroideryCraft\fonts`
  (paste that into the Explorer address bar). The **Fonts** menu shows the exact folder. Copy
  `.ecfont` files there to move fonts from another computer.
- **Projects** (`.embcraft`) and **DST files**: wherever you save them.
- **Background pictures** are linked, not copied into the project: keep the picture next to the
  project if you move it to another computer, then use **Replace…** to point at it again.

## Fonts
Windows fonts appear in the lettering window under their real names. For Japanese and Korean,
**Yu Gothic**, **Meiryo** and **Malgun Gothic** come with Windows; **Noto Sans CJK** (free, SIL
Open Font License) can be installed from Google Fonts. Convert a font once with
**Fonts › Make an EmbroideryCraft font…** for the best satin.

Projects made on another computer refer to fonts by file path. Lettering in them sews as it is,
but retyping it needs the same font on this computer.

## Building it (developers, on Linux)
```
tools/build-windows.sh
```
cross-compiles with `cargo-zigbuild` and writes `dist/EmbroideryCraft-<version>-windows-x64.zip`.
