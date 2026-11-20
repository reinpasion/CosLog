# spec/tools

How `parts.json` was built and how to re-check it. Node 18+, no packages.

| Script | Purpose |
|---|---|
| `load-legacy.mjs` | Evaluates the legacy tables and `sym()` from the original `index.html` (the pre-spec JS). Set `LEGACY_HTML` to point at that file, e.g. `git show 75f6205:index.html`. |
| `gen.mjs` | One-time bootstrap: records every `sym()` call on a mock canvas, folds state variants into `when` layers, merges the nine legacy tables into one record per part, and writes `../parts.json`. After the migration `parts.json` is edited directly; re-running this overwrites it. |
| `verify-symbols.mjs` | Redraws every part from `parts.json` for 28,224 state combinations and compares with the legacy `sym()` output. |
| `verify-rust.mjs` | Checks names and pin positions against `coslogx/circuit_core/src/kind.rs`. |
| `tolerance-vectors.mjs` | Prints the splitmix64 reference vectors listed in `project_schema.md`. |
