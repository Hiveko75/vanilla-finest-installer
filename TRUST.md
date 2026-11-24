# How the installer works

1. The window asks the Modrinth API for the Vanilla Finest versions (project `WbX4zjj3`) and offers,
   for each edition and Minecraft version, the newest pack version.
1. The player picks the edition, the Minecraft version and the memory.
1. The installer downloads the `.mrpack` from `cdn.modrinth.com` (no other host is accepted for it) and
   checks it against the SHA-512 that the Modrinth API lists for that file.
1. It removes the downloaded files of the previous install in the same folder that the new version no
   longer has.
1. It downloads every file the pack lists (mods, resource packs, shaders) and checks each one's SHA-512
   and size. Downloads are only accepted from `cdn.modrinth.com`, `github.com`,
   `raw.githubusercontent.com` and `gitlab.com`. Files already present with the right hash are kept.
1. It copies the pack's settings and bundled files (`overrides`). A file the player changed since the
   last install is kept; one the new version no longer has is removed unless the player changed it.
1. On the first install it sets `homefolders.enabled=true` in `config/vanilla-finest-helper.properties`
   (Vanilla Finest Helper then points screenshots, schematics, replays and blueprints at the launcher's
   `.minecraft` folder on the first start).
1. It downloads Fabric's version file from Fabric's metadata server into the launcher's `versions`
   folder. The launcher itself downloads the libraries and uses its own Java.
1. It adds or updates the installation in `launcher_profiles.json` (name, icon, game folder, Fabric
   version, Java arguments). Settings the player added to that installation are kept.
1. It writes `vanilla-finest-installer.json` in the game folder: which files it installed, the hashes
   of the settings files it wrote, and the pack version.

# How Remove works

1. It removes the installation from `launcher_profiles.json`.
1. It deletes every file `vanilla-finest-installer.json` lists (the downloaded files and the files
   copied from the pack, including settings the player changed), Vanilla Finest Helper's settings
   file, and `vanilla-finest-installer.json` itself. Files in `saves` are never deleted.
1. It deletes Fabric's version folder in `versions` if no other installation in the launcher uses
   that version.
1. It deletes the folders left empty, and the game folder if nothing is left in it. Worlds,
   screenshots, logs, `options.txt` and every other file the installer didn't write stay.

# Trust & security

- File paths in the pack are checked so nothing is written outside the game folder.
- Every download is checked against the hash in the pack, and the pack against Modrinth's hash.
- The installer is built from this repository by GitHub Actions; the built files carry a build
  provenance attestation.
