<script lang="ts">
	import {
		get_installed_metadata,
		get_total_memory,
		install_mrpack,
		is_launcher_installed,
		type InstalledMetadata
	} from '$lib/installer';
	import { get_project, list_versions, PROJECT_ID, PROJECT_URL } from '$lib/modrinth';
	import {
		EDITIONS,
		JAVA_FLAGS,
		installId,
		installName,
		memoryRange,
		packChoices,
		type Edition,
		type PackChoice
	} from '$lib/packs';
	import { trans, locale, langIds, langName, dir, getDir } from '$lib/i18n';
	import { listen } from '@tauri-apps/api/event';
	import { UserAttentionType, appWindow } from '@tauri-apps/api/window';
	import { confirm } from '@tauri-apps/api/dialog';
	import { open } from '@tauri-apps/api/shell';
	import LightThemeIcon from '@fluentui/svg-icons/icons/weather_sunny_24_regular.svg?raw';
	import DarkThemeIcon from '@fluentui/svg-icons/icons/dark_theme_24_regular.svg?raw';
	import DeviceThemeIcon from '@fluentui/svg-icons/icons/laptop_24_regular.svg?raw';
	import TranslateIcon from '@fluentui/svg-icons/icons/local_language_24_regular.svg?raw';
	import HelpIcon from '@fluentui/svg-icons/icons/question_circle_24_regular.svg?raw';
	import {
		Listbox,
		ListboxButton,
		ListboxOptions,
		ListboxOption
	} from '@rgossiaux/svelte-headlessui';

	let totalMods = Infinity;

	listen('install:progress', (event) => {
		const payload = event.payload as (string | number)[];
		if (payload[1] != 'start') return;
		switch (payload[0]) {
			case 'load_pack':
				installProgress = $trans('progress.load_pack');
				currentStep = 1;
				break;
			case 'clean_old':
				installProgress = $trans('progress.clean_old');
				break;
			case 'download_files':
				installProgress = $trans('progress.download_files');
				totalMods = payload[2] as number;
				break;
			case 'download_file':
				installProgress = $trans('progress.download_file', {
					file: payload[3],
					idx: (payload[2] as number) + 1,
					total: totalMods
				});
				currentStep = (payload[2] as number) + 2;
				break;
			case 'extract_overrides':
				installProgress = $trans('progress.extract_overrides');
				currentStep = totalMods + 2;
				break;
			case 'install_loader':
				installProgress = $trans('progress.install_loader');
				currentStep = totalMods + 3;
				break;
			case 'add_profile':
				installProgress = $trans('progress.add_profile');
				currentStep = totalMods + 4;
				break;
		}
	});

	// Versions from Modrinth: per edition, the newest pack for each Minecraft version.
	let choices: Record<Edition, PackChoice[]> | undefined = undefined;
	let loadError: string | undefined = undefined;
	let edition: Edition = 'vanilla';
	let mcVersion = '';
	list_versions(PROJECT_ID)
		.then((versions) => {
			choices = packChoices(versions);
			mcVersion = choices[edition][0]?.mcVersion ?? '';
		})
		.catch((e) => (loadError = String(e)));

	$: editionChoices = choices ? choices[edition] : [];
	$: if (choices && !editionChoices.some((c) => c.mcVersion == mcVersion)) {
		mcVersion = editionChoices[0]?.mcVersion ?? '';
	}
	$: choice = editionChoices.find((c) => c.mcVersion == mcVersion);
	$: name = choice ? installName(edition, choice.mcVersion) : '';

	// Memory: Vanilla at least 6 GB, Creative 8 to 16 GB, and 2 GB left for the system.
	let totalGb: number | undefined = undefined;
	get_total_memory()
		.then((bytes) => (totalGb = bytes / 1024 ** 3))
		.catch(() => (totalGb = undefined));
	let memory = 6;
	$: range = memoryRange(edition, totalGb);
	$: if (memory < range.min) memory = range.min;
	$: if (memory > range.max) memory = range.max;
	$: lowMemory = totalGb !== undefined && Math.floor(totalGb) - 2 < range.min;

	// An earlier install in the same folder: shown, and its memory setting is kept.
	let installed: Partial<InstalledMetadata> | null = null;
	$: loadInstalled(name);
	async function loadInstalled(folder: string) {
		installed = null;
		if (!folder) return;
		try {
			const meta = await get_installed_metadata(folder);
			if (folder != name) return;
			installed = meta;
			if (meta?.memory_gb) memory = meta.memory_gb;
		} catch {
			installed = null;
		}
	}

	function confirmUnload(ev: BeforeUnloadEvent) {
		ev.preventDefault();
		return (ev.returnValue = $trans('ui.confirm-exit'));
	}

	async function installPack() {
		if (!choice) return;
		if (state != 'noLauncher' && !(await is_launcher_installed())) {
			state = 'noLauncher';
			return;
		}
		const picked = choice;
		const folder = name;
		addEventListener('beforeunload', confirmUnload);
		const unlisten = await appWindow.onCloseRequested(async (ev) => {
			const confirmed = await confirm($trans('ui.confirm-exit'));
			if (!confirmed) ev.preventDefault();
		});
		try {
			state = 'installing';
			installedName = folder;
			let icon: Blob | undefined = undefined;
			try {
				const project = await get_project(PROJECT_ID);
				if (project.icon_url) icon = await (await fetch(project.icon_url)).blob();
			} catch {
				icon = undefined;
			}
			await install_mrpack(
				{
					url: picked.file.url,
					sha512: picked.file.hashes.sha512!,
					profileId: installId(edition, picked.mcVersion),
					profileName: folder,
					gameDir: folder,
					javaArgs: `-Xmx${memory}G ${JAVA_FLAGS}`,
					sharedFolders: true,
					metadata: {
						edition,
						mc_version: picked.mcVersion,
						pack_version: picked.version.version_number,
						version_id: picked.version.id,
						memory_gb: memory
					}
				},
				icon
			);
			state = 'postInstall';
			await appWindow.requestUserAttention(UserAttentionType.Informational);
		} catch (e) {
			state = 'error';
			errorMessage = String(e);
			console.error(e);
		} finally {
			removeEventListener('beforeunload', confirmUnload);
			unlisten();
		}
	}

	let state: 'preInstall' | 'installing' | 'postInstall' | 'error' | 'noLauncher' = 'preInstall';
	let installProgress = '';
	let installedName = '';
	$: totalSteps = totalMods + 4;
	let currentStep = 0;
	let errorMessage: string | undefined = undefined;

	const theme_icon_map = {
		'device-theme': DeviceThemeIcon,
		'light-theme': LightThemeIcon,
		'dark-theme': DarkThemeIcon
	};
	const themes: ('device-theme' | 'light-theme' | 'dark-theme')[] = [
		'device-theme',
		'light-theme',
		'dark-theme'
	];
	let theme: 'device-theme' | 'light-theme' | 'dark-theme' = 'device-theme';
	$: document.body.className = theme;
	function cycle_theme() {
		theme = themes[(themes.indexOf(theme) + 1) % 3];
	}

	function reset_state() {
		currentStep = 0;
		totalMods = Infinity;
		state = 'preInstall';
		loadInstalled(name);
	}

	function openHelp() {
		open(PROJECT_URL);
	}

	const buttonClass =
		'rounded-full bg-blue text-base disabled:bg-surface0 py-2 px-4 focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue disabled:text-overlay0';

	$: document.querySelector('html')!.lang = $locale;
	$: document.querySelector('html')!.dir = $dir;
</script>

<div class="absolute top-0 start-0 m-4 fill-text flex flex-row">
	<button class="p-2 hover:bg-surface0 rounded" on:click={cycle_theme}>
		{@html theme_icon_map[theme]}
	</button>
	<Listbox bind:value={$locale}>
		<ListboxButton class="p-2 hover:bg-surface0 rounded">{@html TranslateIcon}</ListboxButton>
		<div class="relative">
			<ListboxOptions
				class="absolute top-1 bg-surface0 rounded shadow-lg text-text flex flex-col max-h-[70vh] overflow-y-scroll z-10"
			>
				{#each langIds as lang}
					<ListboxOption
						value={lang}
						class={({ active }) =>
							active ? 'bg-surface1 p-2 rounded text-nowrap' : 'p-2 rounded text-nowrap'}
						dir={getDir(lang)}
						{lang}
					>
						{langName(lang)}
					</ListboxOption>
				{/each}
			</ListboxOptions>
		</div>
	</Listbox>
</div>
<div class="absolute top-0 end-0 m-4 fill-text">
	<button class="p-2 hover:bg-surface0 rounded" title={$trans('ui.help-tooltip')} on:click={openHelp}>
		{@html HelpIcon}
	</button>
</div>

<div class="flex items-center justify-center w-full h-full bg-base text-text">
	<div class="flex flex-col gap-4 w-[28rem] max-w-[90vw]">
		<img src="/vanilla-finest-logo.png" alt="Vanilla Finest" class="w-full" draggable="false" />
		{#if state == 'preInstall'}
			{#if loadError}
				<div class="text-center text-red">{$trans('ui.load-error', { errorMessage: loadError })}</div>
			{/if}
			<div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label={$trans('ui.edition')}>
				{#each EDITIONS as e}
					<button
						role="radio"
						aria-checked={edition == e}
						class="rounded-md border py-2 px-3 text-center {edition == e
							? 'border-blue bg-surface0 font-semibold'
							: 'border-surface1 hover:bg-surface0'}"
						on:click={() => (edition = e)}>{$trans(`ui.edition-${e}`)}</button
					>
				{/each}
			</div>
			<div class="flex flex-row gap-2 items-center">
				<label for="mc-version" class="whitespace-nowrap">{$trans('ui.minecraft-version')}</label>
				<select
					id="mc-version"
					class="input-box"
					bind:value={mcVersion}
					disabled={choices == undefined || editionChoices.length == 0}
					lang="en"
					dir="ltr"
				>
					{#if choices == undefined}
						<option value="">{$trans('ui.loading-versions')}</option>
					{:else if editionChoices.length == 0}
						<option value="">{$trans('ui.no-versions')}</option>
					{:else}
						{#each editionChoices as c}
							<option value={c.mcVersion}
								>{c.mcVersion} · {c.version.version_number}{c.version.version_type != 'release'
									? ` (${c.version.version_type})`
									: ''}</option
							>
						{/each}
					{/if}
				</select>
			</div>
			<div class="flex flex-col gap-1">
				<label for="memory" class="flex flex-row justify-between">
					<span>{$trans('ui.memory')}</span>
					<span class="font-semibold" dir="ltr">{memory} GB</span>
				</label>
				<input
					id="memory"
					type="range"
					class="w-full accent-blue"
					min={range.min}
					max={range.max}
					step="1"
					bind:value={memory}
					disabled={range.min == range.max}
				/>
				<div class="text-sm text-subtext0">
					{#if lowMemory}
						<span class="text-peach"
							>{$trans('ui.memory-low', {
								total: Math.round(totalGb ?? 0),
								min: range.min,
								edition: $trans(`ui.edition-${edition}`)
							})}</span
						>
					{:else if totalGb !== undefined}
						{$trans('ui.memory-hint', { total: Math.round(totalGb) })}
					{/if}
				</div>
			</div>
			{#if choice}
				<div class="text-sm text-subtext0">
					{$trans('ui.target', { folder: name, name })}
					{#if installed?.pack_version}
						<br />{$trans('ui.update', { version: installed.pack_version })}
					{/if}
				</div>
			{/if}
			<button class={buttonClass} on:click={installPack} disabled={!choice}
				>{installed?.pack_version ? $trans('ui.update-button') : $trans('ui.install-button')}</button
			>
		{:else if state == 'installing'}
			<div class="text-center text-lg">{$trans('ui.installing', { name: installedName })}</div>
			<progress class="progress w-full" value={currentStep / totalSteps} />
			<div class="text-ellipsis whitespace-nowrap overflow-hidden">
				{installProgress}
			</div>
		{:else if state == 'postInstall'}
			<div class="text-center text-lg">{$trans('ui.installed', { name: installedName })}</div>
			<button class={buttonClass} on:click={reset_state}>{$trans('ui.back-home')}</button>
		{:else if state == 'error'}
			<div class="text-center text-lg text-red">
				{$trans('ui.install-error', { errorMessage })}
			</div>
			<button class={buttonClass} on:click={reset_state}>{$trans('ui.back-home')}</button>
		{:else if state == 'noLauncher'}
			<div>
				{$trans('ui.no-launcher')}
			</div>
			<button class={buttonClass} on:click={() => (state = 'preInstall')}
				>{$trans('ui.no-launcher-back')}</button
			>
			<button class={buttonClass} on:click={installPack} disabled={!choice}
				>{$trans('ui.no-launcher-continue')}</button
			>
		{/if}
	</div>
</div>
