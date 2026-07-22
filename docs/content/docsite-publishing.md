# Documentation Site Publishing

Fortemi publishes this documentation site with `@pagenary/publisher`.
The root `package.json` keeps the publisher as a dev dependency and the root
`tenants.json` registers the `fortemi-docs` tenant.

## Current Integration

| Component | Value |
|-----------|-------|
| Publisher package | `@pagenary/publisher` `^2026.7.12` |
| Tenant id | `fortemi-docs` |
| Tenant source | `./docs` |
| Build output | `./dist/fortemi-docs` |
| Production host | `docs.fortemi.com/server` |
| Local config | `docs/config.json` |
| Tenant registry | `tenants.json` |

The package is consumed from npm; CI does not clone the Pagenary source
repository. `npm ci` installs the locked release, then `npm run docs:build`
builds the tenant.

## Local Commands

```bash
npm ci
npm run docs:build
npx pagenary doctor --json
npx pagenary tenants list --json
npx pagenary check content
npx pagenary check media
npx pagenary serve
```

`npm run docs:build` runs:

```bash
pagenary build:tenants fortemi-docs --base auto
```

`--base auto` keeps the same static bundle portable across `/`, `/server`, and
other subpath mounts by letting the runtime resolve its served base path.

## Pagenary 2026.7.12 Capabilities Integrated

- Multi-tenant builds through the root `tenants.json` registry.
- Strict internal link validation through the tenant `strictLinks` setting.
- Collection post materialization for `docs/content/posts/` via
  `docs/config.json`.
- Root HTML fallback through `seo.rootHtmlFallback: true`, embedding the default
  page content directly in `index.html` for bots, no-JavaScript readers, and
  assistive technology before the SPA runtime loads.
- Static SEO artifacts including `sitemap.xml`, `robots.txt`, `llms.txt`, and
  page snapshots.
- Build diagnostics through `pagenary doctor`.
- Consumer-safe quality checks through `pagenary check content` and
  `pagenary check media`.
- Subpath-safe builds with `--base auto`.

The full `pagenary check` suite also exposes SEO, accessibility, reading, and
narration checks. In this Fortemi consumer repo, `npm run docs:build` is the
authoritative tenant validation. Some full-suite targets still assume
Pagenary's source-repository example registry or default `dist/index.html`, so
do not gate Fortemi docs changes on bare `npx pagenary check` until those
targets accept the root tenant registry/output path.

## CI Workflows

`.gitea/workflows/docsite-build.yml` validates the documentation site on docs,
tenant, or package changes. It installs the locked package with `npm ci`, runs
`npm run docs:build`, and verifies that `dist/fortemi-docs/index.html` and the
generated sections directory exist.

`.gitea/workflows/docsite-deploy.yml` runs on release tags, manual dispatch, and
main-branch docsite changes.
It builds the same tenant, deploys `dist/fortemi-docs/` to the server-docs
subpath, and deploys `docs-portal/` to the docs root without deleting the
existing `/server` and `/react` docbases. After deploy, it purges the main
Cloudflare docs URLs when `CLOUDFLARE_ZONE_ID` and `CLOUDFLARE_API_TOKEN` are
configured as repository secrets.

The Cloudflare token needs permission to purge cache for the docs zone. If the
secrets are absent, the workflow logs a skip message and still completes the
server deploy.

The root portal at `docs.fortemi.com/` is hand-authored in `docs-portal/`.
It ships static HTML plus root-level `sitemap.xml`, `robots.txt`, and
`llms.txt` files that route bots and readers to the two published docsites:
`/server/` and `/react/`.

## Scheduled Post Releases

Queued future posts live outside the live docs content tree so normal builds do
not publish them early:

```text
scheduled-docs/posts/<slug>.md
scheduled-docs/assets/blog/<asset>
```

Set a valid ISO-8601 `publish_at` timestamp when a post is ready to release:

```yaml
publish_at: "2026-07-28T14:00:00Z"
scheduled_assets: ["blog/example-hero.png", "blog/example-diagram.svg"]
```

Blank, missing, or invalid `publish_at` values are ignored. The scheduled
`.gitea/workflows/scheduled-docs-release.yml` job runs
`scripts/docs/promote-scheduled-posts.mjs`, moves due posts into
`docs/content/posts/`, moves declared assets into `docs/.public/`, validates
with `npm run docs:build`, commits the promotion, and pushes to `main`. If
nothing is due, the job exits successfully without committing. The existing
docsite deploy workflow then publishes through the normal `/server` route.

## Update Procedure

1. Check the published version:
   ```bash
   npm view @pagenary/publisher version
   ```
2. Update the package and lockfile:
   ```bash
   npm install --save-dev @pagenary/publisher@<version>
   ```
3. Verify the installed CLI:
   ```bash
   npx pagenary version --json
   npx pagenary doctor --json
   npx pagenary check content
   npx pagenary check media
   npm run docs:build
   ```
4. Update this page if commands, tenant behavior, SEO output, or CI assumptions
   changed.

## Accessibility and Bot Access

Keep `docs/config.json` configured with:

```json
{
  "seo": {
    "rootHtmlFallback": true
  }
}
```

The build should log `embedded root HTML fallback: welcome-overview`. After a
build, inspect `dist/fortemi-docs/index.html` to confirm it contains real
default-page content inside `<main id="app">` rather than an empty SPA mount.

## Troubleshooting

If the build fails on internal links, keep `strictLinks` enabled and fix the
source Markdown or manifest entry. For release publishing issues, verify the
generated files locally first, then inspect the deploy workflow output and the
target server path configured by `DOCSITE_DEPLOY_PATH`.
