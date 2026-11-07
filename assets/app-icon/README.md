# Mirai Reader app icon

<img src="pdfcraft-small.svg" alt="Mirai Reader app icon: a purple page with an eye on a deep night-purple tile" width="128">

**Subject ("oceano morado"):** an upright page with a folded corner, an eye and two text lines.

**Style:** flat shapes, no outline; the page has a diagonal purple gradient. No fonts or images.

| Colour | Hex | Used for |
|---|---|---|
| Night purple | `#24183e` | the full-bleed field |
| Lilac to Mirai purple | `#d2a3ff` → `#7042c7` | the page gradient |
| Pale lilac | `#eddaff`, `#f2e7ff` | the folded corner, text lines |
| White | `#ffffff` | the eye |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=118`. Windows and Linux icons use the
full-bleed tile. macOS icons put it on Apple's grid (an 824 px body centred on a transparent 1024 px
canvas).

**Provenance:** original work for the Mirai fork, drawn directly as SVG. It replaces upstream's engraved
lion, which the Mirai fork does not ship. The PNG, `.ico` and `.icns` files were rendered from a 1024 px
raster of the master, since the gradient needs a full SVG renderer (resvg handles it).
Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo).

## Files

| File | What it is |
|---|---|
| `pdfcraft.svg` | the master vector; every PNG, `.ico` and `.icns` is rendered from it |
| `pdfcraft-small.svg` | a copy of the master, kept as its own file for the README and the in-app mark |
| `pdfcraft-1024.png` | 1024 px on Apple's grid; also the runtime Dock icon on macOS |
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
packaging/icons.sh        # needs resvg and python3; iconutil (macOS) for the .icns
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
