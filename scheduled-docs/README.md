# Scheduled docs

Future Fortémi server docs posts live here until the release workflow promotes them into the live docs tree. This keeps queued material out of normal docsite builds.

- Blog posts: `scheduled-docs/posts/*.md`
- Post assets: `scheduled-docs/assets/blog/*`
- Required frontmatter: `publish_at` as an ISO-8601 timestamp when ready to release
- Optional frontmatter: `scheduled_assets` as an inline array of asset paths, relative to `scheduled-docs/assets/`
- Blank, missing, or invalid `publish_at` values are ignored
- Promotion script: `scripts/docs/promote-scheduled-posts.mjs`
- Release workflow: `.gitea/workflows/scheduled-docs-release.yml`

When a post is due, the workflow moves it to `docs/content/posts/`, moves its declared assets into `docs/.public/`, validates the docsite build, commits the promotion, and pushes to `main`.
