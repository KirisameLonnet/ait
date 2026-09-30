# Repository guidance

Before changing domain boundaries, read `docs/README.md`.

- Rust is the fixed implementation language; keep the root Cargo workspace buildable.
- Before any Rust change, read and follow [the Rust style guide](docs/policy/rust.md). All Rust changes, including tests and refactors, MUST comply with it.
- Dependencies point inward: adapters implement ports; application coordinates domain behavior.
- Never commit credentials, provider tokens, local SQLite databases, or runtime artifacts.
- Run format and lint checks appropriate to the changed files before handing off changes.
- During local iteration, run only tests for the changed code and directly related behavior. Run full test suites only when preparing a commit; workspace-wide coverage also counts as a full test run.
- If the current task changes no Rust code in `bins/` or `crates/`, skip Rust tests.
- Record durable boundary changes as an ADR and update `docs/README.md`.
