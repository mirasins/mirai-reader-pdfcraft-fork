# Mirai Reader app icon

<img src="pdfcraft-small.svg" alt="Mirai Reader app icon: an open book on a violet tile" width="128">

The **Mirai Reader icon**: an open book on a glossy violet tile, from the Mirai icon set supplied by
the fork's owner (MIT OR Apache-2.0). It replaces upstream's engraved lion, which the Mirai fork does
not ship.

**Tile:** `viewBox="0 0 512 512"`, a rounded square of 418 px (`rx=116`) centred with a transparent
margin, so it already sits on Apple's icon grid: every platform uses it as is.

**Masters:** `pdfcraft.svg` and `pdfcraft-1024.png`, both as supplied. The smaller PNGs, the `.ico`
and the `.icns` are scaled from the PNG (the SVG's gradients and transparency need a full renderer).
Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo).

## Files

| File | What it is |
|---|---|
| `pdfcraft.svg` | the master vector, as supplied |
| `pdfcraft-small.svg` | a copy of the master, kept as its own file for the README and the in-app mark |
| `pdfcraft-1024.png` | the 1024 px master, as supplied; every smaller PNG, the `.ico` and the `.icns` are scaled from it; also the runtime Dock icon on macOS |
| `pdfcraft.icns` | macOS icon (16–1024 px) |
| `pdfcraft.ico` | Windows icon (16–256 px), embedded in `pdfcraft.exe` by `apps/pdfcraft/build.rs` |
| `pdfcraft-document.svg` | Windows PDF file icon master: a Lucide document outline, red badge and outlined PDF lettering; no font dependency |
| `pdfcraft-document.ico` | Windows PDF file icon (16–256 px), embedded as resource 2 in `pdfcraft.exe` |
| `hicolor/<n>x<n>/apps/ai.storyteller.pdfcraft.png` | Linux hicolor theme, 16–512 px; the 256 px one is the runtime icon on Windows and Linux |
| `hicolor/scalable/apps/ai.storyteller.pdfcraft.svg` | Linux scalable icon (copy of the master) |

Where it shows: `apps/pdfcraft/src/main.rs` sets the window icon (Dock, taskbar, Alt-Tab, launcher) and the
Wayland app id `ai.storyteller.pdfcraft`; `packaging/linux/ai.storyteller.pdfcraft.desktop` names the
hicolor icon.

Windows PDF files use resource 2 (`"pdfcraft.exe",-2`) when PdfCraft is chosen as the default
reader; the app keeps resource 1. The document outline is adapted from Lucide `file-text`
1.49.0 (ISC); its badge and outlined lettering are original (`MIT OR Apache-2.0`), covered by
[LICENSE-document.txt](LICENSE-document.txt). Both icons are embedded, requiring no runtime
sidecar. This provides a file type icon; first-page thumbnails still need a thumbnail provider.

## Regenerate

```sh
packaging/app-icon.sh     # the app icon; needs Python 3 with Pillow
packaging/icons.sh        # the PDF document icon; needs resvg and python3
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
