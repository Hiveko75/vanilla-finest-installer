import { invoke } from '@tauri-apps/api/tauri';

function blobToDataURL(blob: Blob): Promise<string> {
	return new Promise((resolve) => {
		const reader = new FileReader();
		reader.onloadend = () => resolve(reader.result as string);
		reader.readAsDataURL(blob);
	});
}

export interface InstallRequest {
	/** The .mrpack on cdn.modrinth.com. */
	url: string;
	/** Its SHA-512 from the Modrinth API. */
	sha512: string;
	profileId: string;
	profileName: string;
	icon?: string;
	/** Folder inside .minecraft. */
	gameDir: string;
	javaArgs?: string;
	/** Screenshots etc. in .minecraft on the first install (Vanilla Finest Helper). */
	sharedFolders: boolean;
	metadata: InstalledMetadata;
}

/** Stored in the game folder by the installer. */
export interface InstalledMetadata {
	edition: string;
	mc_version: string;
	pack_version: string;
	version_id: string;
	memory_gb: number;
}

export async function install_mrpack(request: InstallRequest, icon?: Blob): Promise<void> {
	if (icon != undefined) request.icon = await blobToDataURL(icon);
	await invoke('install_mrpack', { request });
}

export async function get_installed_metadata(gameDir: string): Promise<Partial<InstalledMetadata> | null> {
	return await invoke('get_installed_metadata', { gameDir });
}

export async function is_launcher_installed(): Promise<boolean> {
	return await invoke('is_launcher_installed');
}

/** Total memory of the computer in bytes. */
export async function get_total_memory(): Promise<number> {
	return await invoke('get_total_memory');
}

export interface RemoveResult {
	/** The game folder. */
	folder: string;
	/** False when worlds, screenshots or other files the installer didn't write are still in it. */
	folderRemoved: boolean;
}

/** Removes the launcher installation and the files the installer wrote. */
export async function remove_installation(profileId: string, gameDir: string): Promise<RemoveResult> {
	return await invoke('remove_installation', { profileId, gameDir });
}

/** Opens a game folder (inside .minecraft) in the file manager. */
export async function open_game_folder(gameDir: string): Promise<void> {
	await invoke('open_game_folder', { gameDir });
}
