#!/usr/bin/env node
/**
 * sync-docs.mjs — copy repo-root documentation into src/content/docs/.
 *
 * The repo files (docs/update-flow.md, DERIVING.md, ...) are the single
 * source of truth; the copies written here are gitignored build artifacts.
 * Each copy gets Starlight frontmatter (title/description/editUrl), loses its
 * leading H1 (Starlight renders the title), and has relative markdown links
 * rewritten to site routes (for synced pages) or GitHub blob URLs.
 *
 * Runs automatically before `npm run dev` and `npm run build` via pre-hooks.
 * After editing a source doc while `astro dev` is running, re-run
 * `npm run sync-docs` — the dev server picks up the regenerated file.
 *
 * If a repo doc is renamed or moved, update MANIFEST below; a missing source
 * makes this script (and CI) fail loudly.
 */

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const websiteDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(websiteDir, '..');
const contentDir = join(websiteDir, 'src', 'content', 'docs');
const REPO_URL = 'https://github.com/sinisterMage/Arch-silverblue';

const MANIFEST = [
	{
		source: 'docs/update-flow.md',
		dest: 'architecture/update-flow.md',
		route: '/architecture/update-flow/',
		title: 'Update & Rollback Flow',
		description:
			'On-disk layout, the seven-step atomic update flow, and the three mechanisms that make auto-rollback actually trigger.',
	},
	{
		source: 'docs/installing.md',
		dest: 'guides/installing.md',
		route: '/guides/installing/',
		title: 'Install on Real Hardware',
		description:
			'Download the ISO, boot it in UEFI mode, and run the minimal plain-prompt installer.',
	},
	{
		source: 'DERIVING.md',
		dest: 'guides/deriving.md',
		route: '/guides/deriving/',
		title: 'Derive Your Own Distro',
		description:
			'Fork Arch Silverblue into your own branded atomic distro by editing a single config file and rebuilding the ISO.',
	},
	{
		source: 'CONTRIBUTING.md',
		dest: 'project/contributing.md',
		route: '/project/contributing/',
		title: 'Contributing',
		description:
			'Dev setup, the make-based dev loop, load-bearing conventions, and PR guidelines.',
	},
	{
		source: 'SECURITY.md',
		dest: 'project/security.md',
		route: '/project/security/',
		title: 'Security Policy',
		description:
			'How to report vulnerabilities, what is supported, and which behaviors are by design.',
	},
];

// Relative link targets that map to site routes beyond the synced sources
// themselves (README.md is replaced on the site by the getting-started page).
const EXTRA_ROUTES = { 'README.md': '/getting-started/' };

const routeBySource = Object.fromEntries(MANIFEST.map((e) => [e.source, e.route]));

function rewriteLinks(line, sourceDir) {
	return line.replace(/\[([^\]]*)\]\(([^)\s]+)\)/g, (match, text, target) => {
		if (/^([a-z][a-z0-9+.-]*:|#)/i.test(target)) return match; // absolute URL or in-page anchor
		const [path, anchor = ''] = target.split('#');
		const repoPath = posix.normalize(posix.join(sourceDir, path));
		const route = routeBySource[repoPath] ?? EXTRA_ROUTES[repoPath];
		const url = route
			? `${route}${anchor ? `#${anchor}` : ''}`
			: `${REPO_URL}/blob/main/${repoPath}`;
		return `[${text}](${url})`;
	});
}

function transform(entry) {
	const raw = readFileSync(join(repoRoot, entry.source), 'utf8');
	const sourceDir = posix.dirname(entry.source) === '.' ? '' : posix.dirname(entry.source);

	const out = [];
	let inFence = false;
	let h1Stripped = false;
	for (const line of raw.split('\n')) {
		if (/^\s*(```|~~~)/.test(line)) {
			inFence = !inFence;
			out.push(line);
			continue;
		}
		if (inFence) {
			out.push(line);
			continue;
		}
		if (!h1Stripped && /^# /.test(line)) {
			h1Stripped = true;
			continue;
		}
		out.push(rewriteLinks(line, sourceDir));
	}
	if (!h1Stripped) throw new Error(`${entry.source}: expected a leading H1 to strip`);
	if (inFence) throw new Error(`${entry.source}: unbalanced code fence`);
	while (out.length && out[0].trim() === '') out.shift();

	const header = [
		'---',
		`title: ${JSON.stringify(entry.title)}`,
		`description: ${JSON.stringify(entry.description)}`,
		`editUrl: ${JSON.stringify(`${REPO_URL}/edit/main/${entry.source}`)}`,
		'---',
		'',
		`<!-- GENERATED from /${entry.source} by website/scripts/sync-docs.mjs — edit the source, then re-run \`npm run sync-docs\`. -->`,
		'',
		'',
	].join('\n');

	return header + out.join('\n');
}

for (const entry of MANIFEST) {
	const destPath = join(contentDir, entry.dest);
	mkdirSync(dirname(destPath), { recursive: true });
	writeFileSync(destPath, transform(entry));
	console.log(`synced ${entry.source} -> src/content/docs/${entry.dest}`);
}
