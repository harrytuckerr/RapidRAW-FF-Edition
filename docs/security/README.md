# Public evidence and CI security

Public reports are sanitized display copies. They retain historical outcomes, source/artifact IDs and hashes; they are not byte-identical substitutes for the original evidence. The [redaction manifest](public-redaction-manifest-2026-10-16.json) records each original and redacted document's SHA-256 and size. Exact originals were preserved privately before editing. A `<HOME>` or `<DEVICE_SERIAL>` is a display placeholder, not a runnable path or device selector.

Keep personal paths, physical device serials and raw Git index payloads out of public reports. Publish index size/hash/equality instead. Do not remove legitimate license credits, generic temporary paths, icon names containing `@2x`, or historical failures to make a privacy scan appear clean.

## Checks

The `Secret and public metadata checks` workflow runs on push and pull requests with a read-only repository token and no supplied secrets. Checkout is pinned to `11bd71901bbe5b1630ceea73d27597364c9af683` (v4.2.2), fetches full branch/tag history and does not persist credentials. No application, package-manager or dependency build is run by this workflow. Its only repository Python execution is the reviewed metadata guard and its synthetic tests; Gitleaks scans locally. It uploads neither scanner reports nor repository data to an external scanner/artifact service.

Gitleaks 8.30.1 Linux x64 is downloaded only from its official release, and the archive is checked against SHA-256 `551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb` before extraction/execution. That value matches both the official release API asset digest and the checksum list; the list itself hashes to `061476c21adaf5441516f96f185c1a4706a83cd6329b9b38762271b3d4a52fae`.

`.gitleaks.toml` extends the binary's complete default configuration. No rules are disabled, no path/test/document allowlists are added, no baseline is used, and `--ignore-gitleaks-allow` prevents inline allow comments from bypassing the check. `.gitleaksignore` contains only six exact, reviewed historical commit/path/rule/line fingerprints: two prose false positives and four historical Clerk publishable frontend keys. It is not permission to add another key or suppress new findings. [Clerk distinguishes public `pk_*` keys from private `sk_*` keys](https://clerk.com/docs/guides/secure/rotate-api-keys). No key value is reproduced in the exceptions. The original unsuppressed audit remains **exit 1 / six findings**, subsequently classified; it was not a clean scan.

Any new detected finding or scanner failure fails CI; detection cannot guarantee that every possible secret format will be found. This git-mode check examines fetched history, not deleted/unadvertised refs, forks, GitHub caches, every Actions log/artifact or issue attachment. Review warnings and coverage limits rather than inferring universal coverage.

The current-document guard reports file/line/category without values. It checks tracked Markdown/JSON for personal macOS home paths (explicit `runner`, `example`, `test-user` examples are allowed), embedded raw-index base64 fields and RF-shaped serials in device/ADB/serial context. This device rule is deliberately narrow, not an inventory of all device identifiers. Other platforms/serial formats still require review. Generic `<HOME>`, `/private/tmp`, emulator selectors and icon filenames are preserved.

For a separately authorized validator:

```sh
python3 -m unittest discover -s .github/tests -p 'test_public_metadata.py'
python3 .github/scripts/check_public_metadata.py
gitleaks git . --log-opts='--all --full-history' --config=.gitleaks.toml \
  --gitleaks-ignore-path=.gitleaksignore --ignore-gitleaks-allow --redact=100 --exit-code=1
```

## Build and release permissions

Ordinary main/PR/lint/Cobalt workflows use `contents: read` and no inherited secret bundle. Main Android CI now requests a debug artifact, as PR/Cobalt already do, to avoid needing release credentials. This changes the main CI artifact category; it does not change the release profile or application version.

Only the release caller retains `contents: write` for uploading release assets. Only its Android matrix entries explicitly pass `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` and `ANDROID_KEY_BASE64`; the reusable workflow declares these optional names and requires an explicit signing input **and** a release event. A requested release signing step fails if configuration is absent, without printing values. Secrets are passed through environment variables rather than interpolated into shell source. Temporary signing files use restrictive creation permissions and are removed by an always-run cleanup step. This is job-local cleanup, not a hard guarantee after runner loss or forced termination.

All checkout steps disable persisted credentials. Other mutable third-party action tags remain a separate hardening opportunity. Static upload paths select APK/AAB/installers and explicit dependency receipts, not the keystore/properties paths; this is not proof that build logs or packaged contents can never expose data. Workflow behavior and release-signing availability require independent validation.

## Historical copies and remaining decisions

A forward sanitization commit does **not** purge existing branches/tags, forks, caches, downloaded copies or previously published release provenance. No history rewrite, ref deletion, credential rotation, release replacement or GitHub setting change is performed by this patch.

The owner can separately choose to retain historical copies with this notice, publish clearly versioned sanitized provenance with new checksums, or authorize coordinated history/asset removal. Rewriting refs changes commit identities and disrupts existing clones/PRs; replacing assets changes byte identity/checksums and cannot recall downloads. Keep original evidence privately and assess those impacts before acting. Never silently replace a released asset while presenting its old checksum as current.

The integrated security change also updates both npm locks to source-map-js 1.2.2 and backports the official GLib iterator fix without misrepresenting its 0.18.5 version. Ten upstream JavaScript security regressions and four optimized host GLib public-FFI regressions passed. Fresh desktop/Android npm installs and both Vite builds passed; desktop and prepared Android locked Cargo metadata checks passed. The first private Android validation used the wrong copied lock and failed; a corrected, separately recorded validation passed. Two standalone test-lint attempts failed before a function-scoped annotation preserved the intentional nth(0) coverage and strict Clippy passed. Original failed checks remain evidence, not passing results.

Rustls was then updated from 0.23.43 to the official patched 0.23.45 in both Cargo locks for [GHSA-2mjx-qc3c-rqvc](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc). All other package versions and resolved dependency edges stayed unchanged. Four independent optimized in-memory public-API tests passed: aligned client/Acceptor controls and exact rejection of messages packed across an encryption boundary. They used ring+std/TLS 1.3 only, with no network, certificate/full-handshake, application or device acceptance claim.

GitHub Dependabot alerts were enabled and verified separately. No alert was dismissed. Secret scanning and push protection were already enabled. The actual [GitHub security run 38881722250](https://github.com/harrytuckerr/RapidRAW-HT-Edition/actions/runs/38881722250) passed both its full-history secret/current-metadata job and its separate four-test optimized Ubuntu GLib job. Existing TypeScript and ESLint failures are not repaired or relabelled by this change. The published preview binaries and tag remain unchanged and predate these source fixes; no new APK/DMG or provider/device acceptance is claimed. Branch protection and removal of historical metadata remain separate decisions.

Sources checked on 2026-10-16: [Gitleaks release](https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1), [official checksum list](https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_checksums.txt), [pinned Gitleaks configuration/CLI documentation](https://github.com/gitleaks/gitleaks/blob/v8.30.1/README.md), [checkout v4.2.2](https://github.com/actions/checkout/tree/11bd71901bbe5b1630ceea73d27597364c9af683).
