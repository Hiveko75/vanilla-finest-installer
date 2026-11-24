import { getVersion } from '@tauri-apps/api/app';

/** Vanilla Finest on Modrinth (https://modrinth.com/modpack/vanilla-finest-survival). */
export const PROJECT_ID = 'WbX4zjj3';
export const PROJECT_URL = 'https://modrinth.com/modpack/vanilla-finest-survival';

export interface Project {
	icon_url?: string;
}

export interface Version {
	name: string;
	id: string;
	game_versions: string[];
	loaders: string[];
	featured: boolean;
	files: VersionFile[];
	version_number: string;
	version_type: 'release' | 'beta' | 'alpha';
	date_published: string;
}

export interface VersionFile {
	url: string;
	primary: boolean;
	filename: string;
	size: number;
	hashes: { sha512?: string; sha1?: string };
}

async function userAgent(): Promise<string> {
	return `VanillaFinestInstaller/${await getVersion()} (+https://github.com/Hiveko75/vanilla-finest-installer)`;
}

export async function get_project(id: string): Promise<Project> {
	const resp = await fetch(`https://api.modrinth.com/v2/project/${id}`, {
		headers: { 'User-Agent': await userAgent() }
	});
	if (!resp.ok) throw new Error(`Modrinth answered ${resp.status}`);
	return await resp.json();
}

export async function list_versions(id: string): Promise<Version[]> {
	const resp = await fetch(
		`https://api.modrinth.com/v2/project/${id}/version?include_changelog=false`,
		{
			cache: 'no-store',
			headers: { 'User-Agent': await userAgent() }
		}
	);
	if (!resp.ok) throw new Error(`Modrinth answered ${resp.status}`);
	return await resp.json();
}
