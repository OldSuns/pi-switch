# AGENTS

Repository-level rules for this project. Where they conflict with the global `AGENTS.md`, this file wins.

## Before claiming completion

- Run the same checks as CI locally and keep them green:

```bash
npm run check && npm test
```

Equivalent to `cargo fmt -- --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked --lib` (see CONTRIBUTING.md).

- This overrides the global `AGENTS.md` ban on running builds and tests: in this repository you are expected to run the checks above yourself. Installing dependencies, publishing and any other action that changes external state still needs approval first.

## Commits

- Follow the existing Conventional Commits style in lowercase English: `type(scope): summary`; type is one of `feat`, `fix`, `chore`, `perf`, `refactor`, `test`, `docs`, `ci`, and scope names the affected area (`web`, `model`, `sessions`, `config`, `tui`).
- Make the message reasonably detailed: the subject states what changed in one line, and the body explains what, why and the blast radius when it is not obvious. Never stop at a bare subject.

## PRs and external content

- Use English as the primary language when creating PRs, issues, release notes and other external content.

## Version bumps and releases

- After bumping the version, if a push is needed, ask the user whether to push a tag (`v*`) and enter the CI build and npm publish flow.
