//! Installs a Vanilla Finest .mrpack into its own game folder for the official Minecraft Launcher.
//!
//! Based on Fabulously Optimized's installer (Paigaldaja, MIT). Kept free of Tauri types, so the whole
//! install can also be run and tested without the window (see `InstallRequest` and `install`).

use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Read},
    mem::ManuallyDrop,
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, Context};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{config, mrpack};

/// Name of the file in the game folder that remembers what the installer wrote.
pub const META_FILE: &str = "vanilla-finest-installer.json";

/// What the window asks the installer to do.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    /// The .mrpack on cdn.modrinth.com.
    pub url: String,
    /// SHA-512 of the .mrpack, as the Modrinth API lists it (hex).
    pub sha512: String,
    /// Key of the installation in launcher_profiles.json, e.g. "vanilla-finest-26.3".
    pub profile_id: String,
    /// Name of the installation in the launcher, e.g. "Vanilla Finest 26.3".
    pub profile_name: String,
    /// Icon as a data URL (the launcher shows it next to the installation).
    pub icon: Option<String>,
    /// Game folder inside .minecraft, e.g. "Vanilla Finest 26.3".
    pub game_dir: String,
    /// Java arguments for the installation (memory and GC flags).
    pub java_args: Option<String>,
    /// Point screenshots, schematics, replays and blueprints at .minecraft on the first install
    /// (Vanilla Finest Helper's Home folders module).
    pub shared_folders: bool,
    /// Stored in the installer's file (pack version, edition, ...).
    pub metadata: serde_json::Value,
}

/// What the installer wrote last time, kept in the game folder.
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct InstallMeta {
    /// Downloaded files (mods, packs, shaders), relative to the game folder.
    #[serde(default)]
    pub files: Vec<String>,
    /// Files copied from the pack's overrides, with the SHA-256 they had when written. A file whose
    /// content differs from this was changed by the player and is kept on updates.
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
    /// The launcher version the installation starts, e.g. "fabric-loader-0.17.3-26.3".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher_version: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// What Remove did, for the window.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    /// The game folder.
    pub folder: String,
    /// False when files the installer didn't write (worlds, screenshots, logs, ...) are still in it.
    pub folder_removed: bool,
}

/// Progress for the window: (step, "start"/"complete", ...).
pub type Progress<'a> = &'a (dyn Fn(serde_json::Value) + Send + Sync);

struct SelfCleanupFile<'a>(ManuallyDrop<File>, &'a Path);

impl<'a> SelfCleanupFile<'a> {
    async fn new(path: &'a Path) -> tokio::io::Result<Self> {
        Ok(Self(ManuallyDrop::new(File::create(path).await?), path))
    }

    fn finalize(mut self) {
        unsafe { ManuallyDrop::drop(&mut self.0) };
        std::mem::forget(self);
    }
}

impl Drop for SelfCleanupFile<'_> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.0) };
        _ = std::fs::remove_file(self.1);
    }
}

/// The official launcher's folder (.minecraft). `VANILLA_FINEST_LAUNCHER_PATH` overrides it (tests).
pub async fn launcher_path() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("VANILLA_FINEST_LAUNCHER_PATH") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    {
        let path = dirs_data_dir().ok_or(anyhow!("Could not determine APPDATA directory!"))?;
        Ok(path.join(".minecraft"))
    }
    #[cfg(target_os = "macos")]
    {
        let path = dirs_data_dir().ok_or(anyhow!("Could not determine local data directory!"))?;
        Ok(path.join("minecraft"))
    }
    #[cfg(target_os = "linux")]
    {
        let path = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(anyhow!("Could not determine home directory!"))?;
        // the Flatpak launcher keeps its own .minecraft
        let flatpak = path.join(".var/app/com.mojang.Minecraft/.minecraft");
        Ok(if tokio::fs::try_exists(&flatpak).await.unwrap_or(false) {
            flatpak
        } else {
            path.join(".minecraft")
        })
    }
}

#[cfg(target_os = "windows")]
fn dirs_data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn dirs_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

/// The game folder of an installation (a folder name inside .minecraft).
pub async fn game_dir_path(game_dir: &str) -> anyhow::Result<PathBuf> {
    let name = parse_and_sanitize_path(game_dir)
        .filter(|p| p.components().count() == 1)
        .ok_or(anyhow!("Invalid game folder name: {game_dir}"))?;
    Ok(launcher_path().await?.join(name))
}

pub async fn read_meta(game_dir: &Path) -> Option<InstallMeta> {
    let text = tokio::fs::read_to_string(game_dir.join(META_FILE)).await.ok()?;
    serde_json::from_str(&text).ok()
}

fn revalidate_url(url: &str, whitelist: &[&str]) -> anyhow::Result<Url> {
    let mut url = reqwest::Url::parse(url).context("Invalid URL!")?;
    let host = url.host_str().ok_or(anyhow!("URL doesn't have a host?"))?;
    if !whitelist.contains(&host) {
        return Err(anyhow!("Domain not allowed here: {}", host));
    }
    if url.scheme() == "http" {
        url.set_scheme("https")
            .map_err(|()| anyhow!("Failed to upgrade HTTP to HTTPS!"))?;
    }
    if url.scheme() != "https" {
        return Err(anyhow!(
            "Weird scheme, possibly malicious: {}",
            url.scheme()
        ));
    }
    Ok(url)
}

async fn try_download(
    client: &reqwest::Client,
    user_agent: &str,
    url: &str,
    path: &Path,
    expected_hash: &[u8],
    expected_size: usize,
) -> anyhow::Result<()> {
    let url = revalidate_url(url, config::DOWNLOADS_DOMAIN_WHITELIST)?;
    let mut resp = client
        .get(url)
        .header("User-Agent", user_agent)
        .send()
        .await?;
    if resp.status() != StatusCode::OK {
        return Err(anyhow!("Status code was not 200, but {}", resp.status()));
    }
    let mut size = 0usize;
    let mut hasher = sha2::Sha512::new();
    let mut file = SelfCleanupFile::new(path).await?;
    while let Some(chunk) = resp.chunk().await? {
        size += chunk.len();
        if size > expected_size {
            return Err(anyhow!(
                "File is bigger than expected: expected {} bytes, aborting on {} bytes",
                expected_size,
                size
            ));
        }
        hasher.update(&chunk);
        file.0.write_all(&chunk).await?;
    }
    if size < expected_size {
        return Err(anyhow!(
            "File is smaller than expected: expected {} bytes, got {} bytes",
            expected_size,
            size
        ));
    }
    let hash: [u8; 64] = hasher.finalize().into();
    if hash != expected_hash {
        return Err(anyhow!(
            "Wrong hash: got {}, expected {}",
            hex::encode(hash),
            hex::encode(expected_hash)
        ));
    }
    file.0.flush().await?;
    file.finalize();
    Ok(())
}

fn parse_and_sanitize_path(path: &str) -> Option<&Path> {
    if path.contains('\0') {
        return None;
    }
    let path = Path::new(path);
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return None,
            Component::ParentDir => depth = depth.checked_sub(1)?,
            Component::Normal(_) => depth += 1,
            Component::CurDir => (),
        }
    }
    if depth == 0 {
        return None;
    }
    Some(path)
}

/// Relative path with "/" separators, as stored in the installer's file.
fn key(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

async fn file_sha512(path: &Path) -> Option<[u8; 64]> {
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(sha2::Sha512::digest(&bytes).into())
}

async fn file_sha256_hex(path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    Some(hex::encode(sha2::Sha256::digest(&bytes)))
}

async fn install_fabriclike(
    client: &reqwest::Client,
    user_agent: &str,
    launcher: &Path,
    profile_url: String,
    version_name: &str,
) -> anyhow::Result<()> {
    let mut profile_json = client
        .get(profile_url)
        .header("User-Agent", user_agent)
        .send()
        .await?;
    if profile_json.status() != StatusCode::OK {
        return Err(anyhow!("Metadata server did not respond with 200"));
    }
    let version_dir = launcher.join("versions").join(version_name);
    let json_path = version_dir.join(format!("{version_name}.json"));
    let jar_path = version_dir.join(format!("{version_name}.jar"));
    tokio::fs::create_dir_all(&version_dir).await?;
    let mut json_file = SelfCleanupFile::new(&json_path).await?;
    while let Some(chunk) = profile_json.chunk().await? {
        json_file.0.write_all(&chunk).await?;
    }
    json_file.0.flush().await?;
    json_file.finalize();
    if jar_path.is_file() {
        tokio::fs::remove_file(&jar_path).await?;
    }
    Ok(())
}

/// Adds the installation to launcher_profiles.json, or updates it. Settings the player added to an
/// existing installation (resolution, ...) are kept; "created" is kept too.
pub fn set_or_create_profile(
    json: &mut serde_json::Value,
    profile_id: &str,
    profile_name: &str,
    profile_icon: Option<&str>,
    version_id: &str,
    game_dir: &str,
    java_args: Option<&str>,
) -> Option<()> {
    let root = json.as_object_mut()?;
    let profiles = root
        .entry("profiles")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()?;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Iso8601::DEFAULT)
        .ok()?;
    let profile = profiles
        .entry(profile_id.to_string())
        .or_insert_with(|| serde_json::json!({ "created": now }))
        .as_object_mut()?;
    profile.insert("name".into(), profile_name.into());
    profile.insert("type".into(), "custom".into());
    profile.insert("lastUsed".into(), now.into());
    profile.insert("lastVersionId".into(), version_id.into());
    profile.insert("gameDir".into(), game_dir.into());
    if let Some(icon) = profile_icon {
        profile.insert("icon".into(), icon.into());
    }
    if let Some(args) = java_args {
        profile.insert("javaArgs".into(), args.into());
    }
    Some(())
}

const SHARED_FOLDERS_COMMENT: &str =
    "# Set by the Vanilla Finest installer: on the first start, screenshots, schematics, replays and blueprints go to the launcher's .minecraft folder";

/// Sets `homefolders.enabled=true` in Vanilla Finest Helper's settings file, keeping its other lines.
async fn enable_shared_folders(game_dir: &Path) -> anyhow::Result<PathBuf> {
    let rel = PathBuf::from("config").join("vanilla-finest-helper.properties");
    let path = game_dir.join(&rel);
    let old = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            let is_key = t.strip_prefix("homefolders.enabled").map_or(false, |rest| {
                let rest = rest.trim_start();
                rest.starts_with('=') || rest.starts_with(':')
            });
            !is_key && !t.starts_with(SHARED_FOLDERS_COMMENT)
        })
        .map(str::to_owned)
        .collect();
    lines.push(SHARED_FOLDERS_COMMENT.into());
    lines.push("homefolders.enabled=true".into());
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, lines.join("\n") + "\n").await?;
    Ok(rel)
}

pub async fn install(
    req: &InstallRequest,
    user_agent: &str,
    progress: Progress<'_>,
) -> anyhow::Result<()> {
    let launcher = launcher_path().await.context("Could not find the Minecraft Launcher's folder")?;
    let game_dir = game_dir_path(&req.game_dir).await?;
    tokio::fs::create_dir_all(&game_dir)
        .await
        .with_context(|| format!("Could not create {}", game_dir.to_string_lossy()))?;
    let previous = read_meta(&game_dir).await;
    let first_install = previous.is_none();
    let previous = previous.unwrap_or_default();
    let client = reqwest::Client::new();

    // 1. The pack itself, checked against the SHA-512 Modrinth lists for it.
    progress(serde_json::json!(["load_pack", "start"]));
    let url = revalidate_url(&req.url, config::PACK_DOMAIN_WHITELIST)?;
    let response = client
        .get(url)
        .header("User-Agent", user_agent)
        .send()
        .await
        .context("Failed to fetch modpack data")?;
    if response.status() != StatusCode::OK {
        return Err(anyhow!("Server did not respond with 200"));
    }
    let bytes = response.bytes().await.context("Failed to fetch modpack data")?;
    let expected = hex::decode(req.sha512.trim()).context("Invalid modpack hash")?;
    let got: [u8; 64] = sha2::Sha512::digest(&bytes).into();
    if expected.len() != 64 || got[..] != expected[..] {
        return Err(anyhow!(
            "The downloaded modpack does not match the one on Modrinth (SHA-512 {}, expected {})",
            hex::encode(got),
            req.sha512
        ));
    }
    let mut mrpack =
        zip::ZipArchive::new(Cursor::new(bytes)).context("Failed to parse modpack file")?;
    let index: mrpack::PackIndex = serde_json::from_reader(
        mrpack
            .by_name("modrinth.index.json")
            .context("No modrinth.index.json in mrpack?")?,
    )
    .context("modrinth.index.json is invalid")?;
    if index.format_version != 1 {
        return Err(anyhow!("Unknown format version {}", index.format_version));
    }
    if index.game != "minecraft" {
        return Err(anyhow!("Unknown game {}", index.game));
    }
    if index.dependencies.contains_key(&mrpack::PackDependency::Forge) {
        return Err(anyhow!("Forge is currently unsupported"));
    }
    progress(serde_json::json!(["load_pack", "complete"]));

    // 2. Downloaded files no longer in the pack are removed.
    progress(serde_json::json!(["clean_old", "start"]));
    let files: Vec<_> = index
        .files
        .into_iter()
        .filter(|f| {
            !matches!(
                f.env.as_ref().and_then(|e| e.get(&mrpack::EnvType::Client)),
                Some(mrpack::SideType::Unsupported)
            )
        })
        .collect();
    let mut new_paths = HashSet::new();
    for file in &files {
        let path = parse_and_sanitize_path(&file.path)
            .ok_or(anyhow!("Possibly malicious download path: {}", file.path))?;
        new_paths.insert(key(path));
    }
    for old in &previous.files {
        if !new_paths.contains(old) {
            if let Some(path) = parse_and_sanitize_path(old) {
                // a file that can't be removed shouldn't stop the install
                let _ = tokio::fs::remove_file(game_dir.join(path)).await;
            }
        }
    }
    progress(serde_json::json!(["clean_old", "complete"]));

    // 3. Mods, packs and shaders; files already there with the right hash are kept.
    progress(serde_json::json!(["download_files", "start", files.len()]));
    let mut written_files = vec![];
    for (i, file) in files.iter().enumerate() {
        progress(serde_json::json!(["download_file", "start", i, &file.path]));
        let rel = parse_and_sanitize_path(&file.path)
            .ok_or(anyhow!("Possibly malicious download path: {}", file.path))?;
        let path = game_dir.join(rel);
        let hash = hex::decode(file.hashes.get(&mrpack::PackFileHash::Sha512).ok_or(anyhow!(
            "No SHA512 hash for file {}; This violates spec!",
            file.path
        ))?)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let present = file_sha512(&path).await.map_or(false, |h| h[..] == hash[..]);
        if !present {
            let mut last_err = None;
            let mut success = false;
            for url in &file.downloads {
                match try_download(&client, user_agent, url, &path, &hash, file.file_size as usize).await {
                    Ok(()) => {
                        success = true;
                        break;
                    }
                    Err(e) => {
                        last_err.replace(e);
                    }
                }
            }
            if !success {
                return Err(anyhow!(
                    "Download failed for {}: {}",
                    file.path,
                    last_err.map_or("no download link".to_string(), |e| format!("{e:#}"))
                ));
            }
        }
        written_files.push(key(rel));
        progress(serde_json::json!(["download_file", "complete", i, &file.path]));
    }
    progress(serde_json::json!(["download_files", "complete"]));

    // 4. Overrides (configs, bundled packs). A file the player changed since the last install is kept.
    progress(serde_json::json!(["extract_overrides", "start"]));
    let mut overrides: BTreeMap<String, String> = BTreeMap::new();
    let names: Vec<String> = mrpack.file_names().map(str::to_owned).collect();
    for name in &names {
        if name.starts_with("overrides/") && names.contains(&format!("client-{name}")) {
            continue;
        }
        let (rel, buf) = {
            let mut entry = mrpack
                .by_name(name)
                .context("Failed to read configuration file; corrupted mrpack?")?;
            if entry.is_dir() {
                continue;
            }
            let enclosed = entry
                .enclosed_name()
                .ok_or(anyhow!("Possibly malicious config path: {}", entry.name()))?;
            let rel = match enclosed
                .strip_prefix("overrides")
                .or_else(|_| enclosed.strip_prefix("client-overrides"))
            {
                Ok(p) if p.components().next().is_some() => p.to_owned(),
                _ => continue,
            };
            let mut buf = vec![];
            entry
                .read_to_end(&mut buf)
                .context("Failed to read configuration file; corrupted mrpack?")?;
            (rel, buf)
        };
        let k = key(&rel);
        let abs = game_dir.join(&rel);
        if let Some(old_hash) = previous.overrides.get(&k) {
            if let Some(disk_hash) = file_sha256_hex(&abs).await {
                if &disk_hash != old_hash {
                    // changed by the player: keep it, and keep remembering the original hash
                    overrides.insert(k, old_hash.clone());
                    continue;
                }
            }
        }
        if let Some(parent) = abs.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create directories for {}", rel.to_string_lossy())
            })?;
        }
        tokio::fs::write(&abs, &buf)
            .await
            .with_context(|| format!("Failed to write {}", abs.to_string_lossy()))?;
        overrides.insert(k, hex::encode(sha2::Sha256::digest(&buf)));
    }
    // overrides the new version no longer has: removed unless the player changed them
    for (k, old_hash) in &previous.overrides {
        if overrides.contains_key(k) {
            continue;
        }
        if let Some(rel) = parse_and_sanitize_path(k) {
            let abs = game_dir.join(rel);
            if file_sha256_hex(&abs).await.as_deref() == Some(old_hash.as_str()) {
                let _ = tokio::fs::remove_file(abs).await;
            }
        }
    }
    if first_install && req.shared_folders {
        let rel = enable_shared_folders(&game_dir)
            .await
            .context("Failed to write Vanilla Finest Helper's settings")?;
        // Only tracked when the pack itself has this file; otherwise it is left alone on updates.
        if overrides.contains_key(&key(&rel)) {
            if let Some(h) = file_sha256_hex(&game_dir.join(&rel)).await {
                overrides.insert(key(&rel), h);
            }
        }
    }
    progress(serde_json::json!(["extract_overrides", "complete"]));

    // 5. Fabric (or Quilt) in the launcher's versions folder.
    progress(serde_json::json!(["install_loader", "start"]));
    let mc_version = index
        .dependencies
        .get(&mrpack::PackDependency::Minecraft)
        .ok_or(anyhow!("Modpack does not specify Minecraft version"))?;
    let mut version_id = mc_version.clone();
    if let Some(fabric) = index.dependencies.get(&mrpack::PackDependency::FabricLoader) {
        version_id = format!("fabric-loader-{fabric}-{mc_version}");
        let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc_version}/{fabric}/profile/json");
        install_fabriclike(&client, user_agent, &launcher, url, &version_id)
            .await
            .context("Failed to install Fabric")?;
    } else if let Some(quilt) = index.dependencies.get(&mrpack::PackDependency::QuiltLoader) {
        version_id = format!("quilt-loader-{quilt}-{mc_version}");
        let url = format!("https://meta.quiltmc.org/v3/versions/loader/{mc_version}/{quilt}/profile/json");
        install_fabriclike(&client, user_agent, &launcher, url, &version_id)
            .await
            .context("Failed to install Quilt")?;
    }
    progress(serde_json::json!(["install_loader", "complete"]));

    // 6. The launcher installation, then the installer's own file.
    progress(serde_json::json!(["add_profile", "start"]));
    let profiles_path = launcher.join("launcher_profiles.json");
    let mut profiles: serde_json::Value = serde_json::from_str(
        &tokio::fs::read_to_string(&profiles_path)
            .await
            .context("Failed to read launcher profiles")?,
    )
    .context("Failed to parse launcher profiles")?;
    set_or_create_profile(
        &mut profiles,
        &req.profile_id,
        &req.profile_name,
        req.icon.as_deref(),
        &version_id,
        &game_dir.to_string_lossy(),
        req.java_args.as_deref(),
    )
    .ok_or(anyhow!("Could not create launcher profile"))?;
    tokio::fs::write(&profiles_path, serde_json::to_string_pretty(&profiles)?)
        .await
        .context("Failed to write launcher profiles")?;
    let meta = InstallMeta {
        files: written_files,
        overrides,
        launcher_version: Some(version_id.clone()),
        metadata: req.metadata.clone(),
    };
    tokio::fs::write(game_dir.join(META_FILE), serde_json::to_string_pretty(&meta)?)
        .await
        .context("Failed to write installer metadata")?;
    progress(serde_json::json!(["add_profile", "complete"]));
    Ok(())
}

/// Removes an installation: the launcher installation, every file the installer wrote into the game
/// folder (mods, packs, shaders and the pack's settings files), Fabric's version folder when no other
/// installation starts it, and the folders left empty. Worlds, screenshots and every other file the
/// installer didn't write stay; the game folder itself goes only when nothing is left in it.
pub async fn uninstall(profile_id: &str, game_dir_name: &str) -> anyhow::Result<RemoveResult> {
    let launcher = launcher_path().await.context("Could not find the Minecraft Launcher's folder")?;
    let game_dir = game_dir_path(game_dir_name).await?;
    let meta = read_meta(&game_dir).await.unwrap_or_default();
    let mut launcher_version = meta.launcher_version.clone();

    // 1. The launcher installation.
    let profiles_path = launcher.join("launcher_profiles.json");
    let mut version_still_used = false;
    if let Ok(text) = tokio::fs::read_to_string(&profiles_path).await {
        let mut json: serde_json::Value =
            serde_json::from_str(&text).context("Failed to parse launcher profiles")?;
        if let Some(profiles) = json.get_mut("profiles").and_then(|p| p.as_object_mut()) {
            let removed = profiles.remove(profile_id);
            if launcher_version.is_none() {
                launcher_version = removed
                    .as_ref()
                    .and_then(|p| p.get("lastVersionId"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
            }
            if let Some(v) = &launcher_version {
                version_still_used = profiles
                    .values()
                    .any(|p| p.get("lastVersionId").and_then(|x| x.as_str()) == Some(v.as_str()));
            }
            if removed.is_some() {
                tokio::fs::write(&profiles_path, serde_json::to_string_pretty(&json)?)
                    .await
                    .context("Failed to write launcher profiles")?;
            }
        }
    }

    // 2. The files the installer wrote. Worlds are never touched.
    for k in meta.files.iter().chain(meta.overrides.keys()) {
        if k.starts_with("saves/") {
            continue;
        }
        if let Some(rel) = parse_and_sanitize_path(k) {
            let _ = tokio::fs::remove_file(game_dir.join(rel)).await;
        }
    }
    let _ = tokio::fs::remove_file(game_dir.join(META_FILE)).await;
    // Vanilla Finest Helper's settings (the installer wrote them on the first install; the mods that
    // used them are gone now).
    let _ = tokio::fs::remove_file(game_dir.join("config").join("vanilla-finest-helper.properties")).await;

    // 3. Fabric's (or Quilt's) version folder, unless another installation starts that version.
    if let Some(v) = launcher_version {
        let loader = v.starts_with("fabric-loader-") || v.starts_with("quilt-loader-");
        if loader && !version_still_used {
            if let Some(name) = parse_and_sanitize_path(&v).filter(|p| p.components().count() == 1) {
                let _ = tokio::fs::remove_dir_all(launcher.join("versions").join(name)).await;
            }
        }
    }

    // 4. Folders left empty, and the game folder when it is empty.
    let dir = game_dir.clone();
    let folder_removed = tokio::task::spawn_blocking(move || {
        remove_empty_dirs(&dir);
        !dir.exists()
    })
    .await
    .unwrap_or(false);
    Ok(RemoveResult {
        folder: game_dir.to_string_lossy().into_owned(),
        folder_removed,
    })
}

/// Removes the empty folders under `dir`, then `dir` itself if it ended up empty. Symbolic links are
/// not followed.
fn remove_empty_dirs(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut empty = true;
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if !(is_dir && remove_empty_dirs(&entry.path())) {
            empty = false;
        }
    }
    empty && std::fs::remove_dir(dir).is_ok()
}

/// Opens the game folder in the system's file manager.
pub async fn open_game_folder(game_dir_name: &str) -> anyhow::Result<()> {
    let dir = game_dir_path(game_dir_name).await?;
    if !dir.is_dir() {
        return Err(anyhow!("{} does not exist", dir.to_string_lossy()));
    }
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .with_context(|| format!("Could not open {}", dir.to_string_lossy()))?;
    Ok(())
}
