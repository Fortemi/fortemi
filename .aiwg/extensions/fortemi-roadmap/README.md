# fortemi-roadmap

Project roadmap advancement: read .aiwg/planning/roadmap.md, advance the next actionable item, document progress

## What this is

A project-local AIWG extension living under `.aiwg/extensions/fortemi-roadmap/`.
Discovered automatically by `aiwg use` and deployed alongside upstream
artifacts.

## Layout

```
.aiwg/extensions/fortemi-roadmap/
├── manifest.json          # Bundle metadata (validated by aiwg)
├── README.md              # This file
└── skills/ or rules/
```

## Usage

Deploy to your configured providers:
```bash
aiwg use fortemi-roadmap
```

Inspect health:
```bash
aiwg doctor --project-local
```

Remove (preserves source under `.aiwg/`):
```bash
aiwg remove fortemi-roadmap
```

## Identical-form portability

This directory is shaped **byte-identical** to upstream
`agentic/code/addons/fortemi-roadmap/`. To graduate, run:

```bash
aiwg promote fortemi-roadmap --dry-run     # preview
aiwg promote fortemi-roadmap                # copy to upstream
aiwg promote fortemi-roadmap --to corpus ~/my-corpus/   # or to a private corpus
```

Keep this directory shaped like upstream so `aiwg promote` works.

## Customization tips

- Edit `manifest.json` to set a real `description`, bump `version` to
  `1.0.0` when stable, and add platforms beyond `claude` if needed.
- Add new artifacts under `skills/`, `rules/`, `agents/`, or `commands/`
  per AIWG conventions.
- Use `@`-references for cross-artifact links: `@$AIWG_ROOT/...` for
  upstream paths, `@.aiwg/...` for project-local references (note: the
  latter will block promotion unless `--force` is passed).

## See also

- `docs/customization/project-local-quickstart.md` — first bundle in 5 minutes
- `docs/customization/project-local-lifecycle.md` — full lifecycle reference
- `docs/customization/extensions-vs-addons-vs-frameworks-vs-plugins.md` — pick the right type
