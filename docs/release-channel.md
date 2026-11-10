# Mirai fork release channel

The Mirai fork ships its own releases, separate from upstream `storytold/pdfcraft`.

## Versioning

- The workspace version in the root `Cargo.toml` is the single source of truth. Change it with
  `cargo xtask version set X.Y.Z-mirai.N`, which updates every workspace dependency and refreshes
  `Cargo.lock`; `cargo xtask version` prints it.
- Fork versions are `MAJOR.MINOR.PATCH-mirai.N`, with `PATCH` above upstream's (upstream
  `0.18.1` → fork `0.18.2-mirai.1`). **Every fork release raises `PATCH` (or more)**:
  `craft-update` treats a `-mirai` build as a local build and offers it only a strictly higher
  `MAJOR.MINOR.PATCH`, so a release that only bumps `N` is never offered to installed copies.
  Windows' MSI `ProductVersion` keeps only the triple as well. `N` stays `1` unless the same triple
  must be rebuilt.
- The tag `v<version>` (e.g. `v0.18.2-mirai.1`) names the GitHub release.

## Update checks and links

`crates/ui-egui/src/lib.rs` defines `UPDATE_APP` with the repository
`mirasins/mirai-reader-pdfcraft-fork`. The in-app updater and the Windows self-update helper both use
it, so update checks and downloads only ever look at the fork's GitHub releases — never at
upstream's. The releases page (`crates/ui-egui/src/platform/updates.rs`) and the source link
(`crates/engine/src/links.rs`) name the fork too. `crates/ui-egui/tests/it/fork_release.rs` pins
all three.

## Cutting a release

Fork releases are built locally; GitHub Actions is not used for them.

1. Land the work on `main` with `cargo xtask ci` green, raising the version as above.
2. Build the Windows packages from the repository root:
   `pwsh -File packaging/windows/package.ps1 -Arch x64` (MSI and portable zip in `dist/release`).
3. Publish them as the release `v<version>` on the fork, marked latest, targeting `main`:
   `gh release create v<version> dist/release/* --target main --latest`.
   GitHub publishes a SHA-256 digest for every asset; the updater refuses a download without one.
4. Packages are unsigned: Windows SmartScreen asks testers to confirm ("More info", "Run anyway").

## Install identity

`craft-update` derives the bundle id from the app id (`ai.storyteller.pdfcraft`), shared with
upstream. The MSI keeps upstream's `UpgradeCode` (`packaging/windows/pdfcraft.wxs`), so installing
the fork's MSI replaces an upstream PdfCraft on the same machine (and the reverse) instead of
installing beside it; the update channel itself is fork-only.
