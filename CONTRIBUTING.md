<!-- Generated from tmtroot/contributing.tmt. Edit that, then `tomet export .`. -->

# Contributing to tomet-search

## Documentation Policy

- All Markdown files (`README.md`, `AGENTS.md`, `CONTRIBUTING.md`, etc.) are generated build artifacts.
Never edit `.md` files directly.
- Always edit the source `.tmt` files under `tmtroot/`.
- Run `just docs` (or `tomet export .`) to regenerate Markdown files, and verify with `just docs-check`.

## Commit Convention

All commits must follow the Conventional Commits format: `type(scope): description`.

### Types

- `feat`: New feature.
- `fix`: Bug fix.
- `docs`: Documentation changes (`.tmt` or generated docs).
- `refactor`: Code restructuring without changing behavior.
- `perf`: Performance improvement.
- `test`: Adding or correcting tests.
- `chore`: Build scripts, dependencies, tooling, repo maintenance.

### Scopes

Use the affected component as the scope:

- **Search**: `search` (`crates/tomet-search`)
- **Infra / Meta**: `nix`, `deps`, `docs`

## Verification

Before finalizing changes:

```bash
just build
just test
just clippy
just docs-check
```

