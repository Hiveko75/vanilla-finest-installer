// Which Vanilla Finest versions the installer offers: per edition and Minecraft version, the newest
// pack version (the newest release when there is one).
import flexver_compare from './flexver';
import type { Version, VersionFile } from './modrinth';

export type Edition = 'vanilla' | 'creative';
export const EDITIONS: Edition[] = ['vanilla', 'creative'];

/** Memory (GB) per edition: the least the game gets, and the most the slider offers. */
export const MEMORY: Record<Edition, { min: number; max: number }> = {
	vanilla: { min: 6, max: 16 },
	creative: { min: 8, max: 16 }
};

/** Java arguments after -Xmx (Hiveko's flags). */
export const JAVA_FLAGS = '-XX:+UseZGC -XX:+ZGenerational -XX:+AlwaysPreTouch -XX:+PerfDisableSharedMem';

export interface PackChoice {
	edition: Edition;
	mcVersion: string;
	version: Version;
	file: VersionFile;
}

/**
 * The edition of a pack version. Version numbers look like "26.3.6.1_Vanilla", "26.3-v7-Creative",
 * older ones "26.2.5_Survival", "26.2.4C" / "26.2.4S" or just "26.2.3" (the Survival edition, now
 * called Vanilla).
 */
export function editionOf(v: Version): Edition {
	const number = v.version_number.toLowerCase();
	if (number.includes('creative') || /\dc$/.test(number) || /creative/i.test(v.name)) {
		return 'creative';
	}
	return 'vanilla';
}

function packFile(v: Version): VersionFile | undefined {
	return (
		v.files.find((f) => f.primary && f.filename.endsWith('.mrpack')) ??
		v.files.find((f) => f.filename.endsWith('.mrpack'))
	);
}

function better(a: PackChoice, b: PackChoice): boolean {
	const aRelease = a.version.version_type === 'release';
	const bRelease = b.version.version_type === 'release';
	if (aRelease !== bRelease) return aRelease;
	return a.version.date_published > b.version.date_published;
}

/** Per edition, one choice per Minecraft version, newest Minecraft version first. */
export function packChoices(versions: Version[]): Record<Edition, PackChoice[]> {
	const best: Record<Edition, Map<string, PackChoice>> = { vanilla: new Map(), creative: new Map() };
	for (const version of versions) {
		if (version.loaders.length > 0 && !version.loaders.includes('fabric')) continue;
		const file = packFile(version);
		if (!file || !file.hashes.sha512) continue;
		const edition = editionOf(version);
		for (const mcVersion of version.game_versions) {
			const choice = { edition, mcVersion, version, file };
			const old = best[edition].get(mcVersion);
			if (!old || better(choice, old)) best[edition].set(mcVersion, choice);
		}
	}
	const sorted = (m: Map<string, PackChoice>) =>
		[...m.values()].sort((a, b) => -flexver_compare(a.mcVersion, b.mcVersion));
	return { vanilla: sorted(best.vanilla), creative: sorted(best.creative) };
}

/** Name of the launcher installation and of its folder inside .minecraft. */
export function installName(edition: Edition, mcVersion: string): string {
	return edition === 'creative' ? `Vanilla Finest ${mcVersion} Creative` : `Vanilla Finest ${mcVersion}`;
}

/** Key of the installation in launcher_profiles.json. */
export function installId(edition: Edition, mcVersion: string): string {
	return edition === 'creative' ? `vanilla-finest-${mcVersion}-creative` : `vanilla-finest-${mcVersion}`;
}

/** The slider's range on a computer with `totalGb` of memory (2 GB stay free for the system). */
export function memoryRange(edition: Edition, totalGb: number | undefined): { min: number; max: number } {
	const { min, max } = MEMORY[edition];
	if (totalGb === undefined) return { min, max };
	return { min, max: Math.max(min, Math.min(max, Math.floor(totalGb) - 2)) };
}
