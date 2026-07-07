// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

// https://astro.build/config
export default defineConfig({
	// Placeholder until the Netlify site (or a custom domain) exists.
	site: 'https://arch-silverblue.netlify.app',
	integrations: [
		starlight({
			title: 'Arch Silverblue',
			description:
				'Atomic, transactional, auto-rolling-back system updates for a fully mutable Arch Linux.',
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/sinisterMage/Arch-silverblue',
				},
			],
			editLink: {
				// Synced pages override this via per-page editUrl frontmatter so
				// "Edit page" points at the repo-root source, not the copy.
				baseUrl: 'https://github.com/sinisterMage/Arch-silverblue/edit/main/website/',
			},
			customCss: ['./src/styles/custom.css'],
			plugins: [starlightLinksValidator()],
			sidebar: [
				{
					label: 'Start',
					items: [
						{ label: 'Getting Started', slug: 'getting-started' },
						{ label: 'FAQ', slug: 'faq' },
						{ label: 'Comparison', slug: 'comparison' },
					],
				},
				{
					label: 'Architecture',
					items: [{ label: 'Update & Rollback Flow', slug: 'architecture/update-flow' }],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Install on Real Hardware', slug: 'guides/installing' },
						{ label: 'Derive Your Own Distro', slug: 'guides/deriving' },
					],
				},
				{
					label: 'Project',
					items: [
						{ label: 'Contributing', slug: 'project/contributing' },
						{ label: 'Security Policy', slug: 'project/security' },
					],
				},
			],
		}),
	],
});
