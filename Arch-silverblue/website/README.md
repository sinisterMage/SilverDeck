# Arch Silverblue website

The project website: landing page + documentation, built with
[Astro](https://astro.build) + [Starlight](https://starlight.astro.build),
deployed to Netlify (config in the repo-root `netlify.toml`).

## Commands (run from `website/`)

| Command             | Action                                                   |
| :------------------ | :------------------------------------------------------- |
| `npm install`       | Install dependencies                                     |
| `npm run dev`       | Dev server at `localhost:4321` (syncs docs first)        |
| `npm run build`     | Production build to `./dist/` (syncs docs, checks links) |
| `npm run preview`   | Preview the production build locally                     |
| `npm run sync-docs` | Re-copy repo docs into `src/content/docs/` (see below)   |

## How the docs stay in sync

The technical docs on the site are **generated copies** of the repo-root
files (`docs/update-flow.md`, `DERIVING.md`, `CONTRIBUTING.md`,
`SECURITY.md`). `scripts/sync-docs.mjs` copies them into
`src/content/docs/`, injecting Starlight frontmatter and rewriting relative
links; the copies are gitignored. **Edit the repo-root files, never the
generated copies.** The script runs automatically before `dev` and `build`.

Web-only pages (landing, getting started, FAQ, comparison) live directly in
`src/content/docs/` and are edited normally.
