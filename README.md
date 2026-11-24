# Vanilla Finest Installer

Installs the [Vanilla Finest](https://modrinth.com/modpack/vanilla-finest-survival) modpacks into the
official Minecraft Launcher, with a simple window: pick the edition (Vanilla or Creative) and the
Minecraft version, set the memory, and click Install.

Based on [Fabulously Optimized's installer](https://github.com/Fabulously-Optimized/vanilla-installer-rust)
by Skye (MIT). Thank you!

## What it does

- Lists the Vanilla Finest versions from Modrinth: for each edition and Minecraft version, the newest
  pack version.
- Installs each one into **its own folder** inside `.minecraft`, named like its launcher installation:
  `Vanilla Finest 26.3` (Vanilla) or `Vanilla Finest 26.3 Creative` (Creative). Worlds, settings and
  mods stay separate from normal Minecraft and from other versions.
- Adds the installation to the Minecraft Launcher, with the Vanilla Finest icon, Fabric, and the Java
  arguments: the memory from the slider (Vanilla at least 6 GB, Creative 8 to 16 GB) and
  `-XX:+UseZGC -XX:+ZGenerational -XX:+AlwaysPreTouch -XX:+PerfDisableSharedMem`.
- On the first install, sets Vanilla Finest Helper to keep screenshots, schematics, replays and
  blueprints in the launcher's `.minecraft` folder, next to the normal Minecraft ones.
- Installing again updates the pack: files the new version no longer has are removed, files that are
  already right are not downloaded again, and settings files the player changed are kept. Files the
  player added (their own mods, worlds, screenshots) are never touched.

Details of every step: [TRUST.md](TRUST.md).

## Download

From the [releases](https://github.com/Hiveko75/vanilla-finest-installer/releases):

- **Windows:** the `.exe`. The installer is not code-signed yet, so Windows may show "Windows protected
  your PC": click *More info*, then *Run anyway*.
- **macOS** (Intel and Apple Silicon): the `.dmg`. Drag the app to Applications; the first time,
  right-click it and choose *Open*.
- **Linux:** the `.AppImage`. Make it executable and run it. The Flatpak launcher's folder is found too.

The official Minecraft Launcher has to be installed and started once before.

## Removing

Open the installer, pick the edition and the Minecraft version, and click **Remove Vanilla Finest …**
under the Update button. It removes the installation from the Minecraft Launcher, the mods, packs and
settings files it installed, and Fabric's version folder if no other installation uses it. Worlds,
screenshots and the other files the game made stay in the folder (*Open the folder* shows it); delete
the folder yourself if you don't need them.

## Building

Every push to `main` builds the three files in GitHub Actions (kept 14 days as build artifacts, with a
build provenance attestation: `gh attestation verify <file> -R Hiveko75/vanilla-finest-installer`).
A release is made by hand: Actions → Release → Run workflow.

Locally: `pnpm install`, then `pnpm tauri dev` or `pnpm tauri build` (Rust, Node 22 and pnpm 10; on
Linux the WebKitGTK 4.0 libraries, see the build workflow).

## Code signing policy

The Windows installer is to be signed through SignPath Foundation (applied for; until the project is
accepted, releases are unsigned):

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/).

- Only the installer built from this repository by the Release workflow on GitHub's own runners is
  signed. Nothing built anywhere else is signed, and neither are the files the installer downloads
  (the modpack, its mods and packs).
- Team roles:
  - Committers and reviewers: [Hiveko](https://github.com/Hiveko75)
  - Approvers: [Hiveko](https://github.com/Hiveko75)
- Every signing request is approved by hand. Changes from other people come as pull requests and are
  reviewed before they are merged. Everyone in these roles uses two-factor authentication on GitHub
  and SignPath.

## Privacy

This program will not transfer any information to other networked systems unless specifically
requested by the user or the person installing or operating it. It collects nothing about you or
your computer. It only connects to:

- `api.modrinth.com`, for the list of Vanilla Finest versions and the project icon;
- the hosts the pack downloads from (`cdn.modrinth.com`, `github.com`, `raw.githubusercontent.com`,
  `gitlab.com`), for the pack and its files;
- `meta.fabricmc.net`, for Fabric's version file.

Its requests carry the user agent `VanillaFinestInstaller/<version>`. On your computer it only changes
the files listed in [TRUST.md](TRUST.md).

## Licence

MIT, see [LICENSE](LICENSE).
