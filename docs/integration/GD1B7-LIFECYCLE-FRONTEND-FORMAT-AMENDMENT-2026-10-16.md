# GD1b7-B frontend test formatting amendment (source only)

Model: claude-haiku-5-5 · 2026-10-16 · Base: detached `abf1ed796246cb952691d287451b843c61985c43` · Worktree: `drive-lifecycle-frontend-format`

Scope: one test file formatted with the installed Prettier 3.9.6. This is source-format custody and AST-equivalence evidence only. It is **not** a test run, a lint or type gate, runtime evidence, or acceptance.

## What changed

- `src/__tests__/applicationLifecycle.test.mjs` only. Original 4723 B, SHA256 `b9822921b8ac1e59e07586e369016199a7b0380790844e1e1f9748915cb7d3fc`. Formatted 5031 B, SHA256 `5f1dff4a6fe7b4093d0d241dd3a9fed1767164230fd4c4df2b97a903ded740d5`.
- The diff is 12 changed opcodes (91 → 139 lines). It contains only line wrapping, trailing commas, parentheses around single arrow parameters, and nested-ternary layout. The patch diff SHA256 is `543e03bf841edbf1da9617c723400ee23425fe31e354b483b8c942cb6ff0648a`.
- The source is **not** byte-identical apart from whitespace. Equivalence is claimed at the AST level only.

## Semantic AST equivalence

The root checker `frontend_format_ast_check.mjs` (SHA256 `ca151c2f4ac8c19edc4f811000a8bfa1f3b5ddd48f5331bc763da432773136d3`, copied byte-exact, unmodified) parses both files with Espree 11.2.0. It does not evaluate or import either file.

- Normalized AST equal: yes. Normalized AST SHA256 `97069e44673f7e58f6e85fe1837a2697b95aba8e0f492aff3bd7cc37af7e22c3`.
- Comment values equal: yes.
- Four top-level test names equal, in order:
  1. `real Settings callback saves then routes all five supported platforms`
  2. `real Settings callback fails closed on unknown platforms or rejected work`
  3. `real external-edit callback routes exit0 without changing finishing state on success`
  4. `real external-edit callback handles unknown/rejected routes without fallback or unhandled rejection`
- The normalization omits only AST position fields and `Literal.raw` syntax. Literal values, regex source and flags, template raw and cooked values, node order, and all other semantic fields are kept.
- The 19 routing rows are not counted separately by any rule. They are covered by the full AST equality above.

## Commands and timings (exact)

Both commands ran with cwd = the worktree and this Node binary: `/Users/harrisontucker/.nvm/versions/node/v22.22.0/bin/node`. Environment: inherited from the shell, with no overrides.

| Step | argv (after the Node path) | UTC start → finish | Duration | Exit | stdout / stderr |
|---|---|---|---|---|---|
| 1. write (once) | `node_modules/prettier/bin/prettier.cjs --write src/__tests__/applicationLifecycle.test.mjs` | 04:52:27.437568 → 04:52:27.700133 | 0.2622 s | 0 | `src/__tests__/applicationLifecycle.test.mjs 96ms` / empty |
| 2. check (once) | `node_modules/prettier/bin/prettier.cjs --check src/__tests__/applicationLifecycle.test.mjs` | 04:52:27.700808 → 04:52:27.956752 | 0.2559 s | 0 | `All matched files use Prettier code style!` / empty |
| 3. AST check (once) | copied checker, args = saved original and formatted file | 04:52:35.927338 (start) | — | 0 | JSON, `ASTequal: true`, `testsExecuted: 0` / empty |

No initial check was rerun. No other file was formatted.

## Custody before and after

- Before any write, all 750 inputs in the binding manifest matched their recorded bytes and SHA256. The manifest itself is `5239fe5eab0b840ac72456824efa298775945a5aa9680fc99410eee5ad2c8732`.
- Git metadata files (`HEAD`, `index`, `pointer`) matched by byte read. No Git command was run.
- The raw index was parsed entry by entry: 713 entries, version 2, all equal to the manifest's `index_entries`. The extension-region SHA256 matched. The index is `eb0fa47e62a7f196232f45198b7528af395b5367baa1d850e7fbef9ee8047632` before and after.
- Footer bindings for `commands.rs` and `lifecycle.rs` matched.
- The `node_modules` alias is a symlink to `/private/tmp/rapidraw-frontend-baseline/node_modules`. It resolves to `/private/tmp/rapidraw-integration-20261015/frontend/node_modules`, as recorded.
- Prettier 3.9.6 and Espree 11.2.0 `package.json` versions match. The formatter and parser entry hashes match the manifest.
- After formatting, 749 of 750 inputs still match the manifest. The only input that differs is the owned test file.

## Deviations and boundaries

- **Not exercised:** test execution (zero tests ran), `tsc`, ESLint, Vite, Cargo, runtime, app, provider, device, and browser. This amendment makes no functional claim.
- **Original Prettier check failure:** retained by reference to the grant. Not rerun, not relabeled.
- **Node binary identity:** checked by reading its file bytes (SHA256 `913b144f…4d4`, matching the prior record). No `--version` or other toolchain query was run.
- **Native cache:** not leased in this task.
- No product, lint, type, or canonical acceptance is claimed. No downstream authority is granted.
