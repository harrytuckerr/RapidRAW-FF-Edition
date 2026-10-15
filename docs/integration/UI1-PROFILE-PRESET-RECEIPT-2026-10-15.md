# UI1 — Camera Profile and adjustment preset clarity

Frozen worker package for independent review. Actual model `gpt-6.1-sol`; isolated source `/Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/profile-preset-ui`, detached base `5ca276ac1dab210f11ee4cecbf904038516164f8`. No commit/stage/push, native build, device, installed-app or release work.

## Result and behavior

Camera Profile now explains the base DCP and optional Cobalt Look before selection. The two visible import actions use separate `.dcp` / `.xmp` filters; current base/Look names and Look amount stay visible. The existing browser receives clearer base/Look group labels; its selection/keyboard/disabled primitive is retained. Loading and list errors are visible. Per-file failures retain original native diagnostics. Only XMP Look-destination failures show adjustment-preset guidance, linked through existing `setPanel(Panel.Presets)`. DCP import failures retain diagnostics without the preset hint or Go to presets action; a real-panel regression covers this lead-review correction. No retry through another importer, fallback or profile substitution.

Adjustment presets now has a visible Import presets action, an exposure/colour/effects layering explanation, active profile context and captured-profile item labels. English configure-modal wording makes the existing profile-capture checkbox explicitly optional. Other15locales remain unchanged and new keys use existing English fallback. No broad Lightroom/Adobe compatibility claim.

A concrete actual-panel regression exposed that turning an ordinary preset off restored an old Camera Profile after a later profile edit. The narrow restore branch now keeps the current `profile` for an ordinary preset, while deliberate captured-profile presets retain the old restore behavior. `mixAdjustments`, the entire intensity handler, and base/Look/amount selection handlers are byte-identical to baseline (preserved-semantics.json). No renderer/parser/camera-pairing/schema/store/math change.

## Validation

| Check                                                                                                   | Status / observed result                                                                              |
| ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Actual components + regressions                                                                         | Pass:46tests =16existing profile +18existing import +12new panel tests,0fail/skip                     |
| Frontend production bundle                                                                              | Pass, exit0; Vite8.2.2 configLoader runner; output only private evidence                              |
| Changed source formatting and diff whitespace                                                           | Pass                                                                                                  |
| Full TypeScript check                                                                                   | Fail inherited:70errors, exit2; same fresh5ca baseline                                                |
| TypeScript introduced-diagnostic comparison                                                             | Pass:0new; file/code/primary-message multiset, ignoring line shifts and i18n-union abbreviation count |
| Scoped TSX lint                                                                                         | Fail inherited:46diagnostics, exit1; baseline also46                                                  |
| Scoped lint introduced-diagnostic comparison                                                            | Pass:0new; file/severity/rule/message multiset                                                        |
| New component test lint                                                                                 | Pass, exit0                                                                                           |
| Local actual-component captures/inspection                                                              | Pass:desktop1440×1000; phone390×1600; phone import-error390×1600                                      |
| Real Tauri transport, native import/renderer, Android/Fold, camera correctness/fidelity, profile export | Not exercised                                                                                         |
| Independent source/visual acceptance                                                                    | Pending reviewer                                                                                      |

The tests click the real import buttons, verify exact filters/cancel/empty states and actual hooks, preserve wrong-destination diagnostics without fallback, show real base/Look summary, exercise the real Look amount slider, retain list errors, apply ordinary presets at0/50/100/200 intensity with independently stated expected exposure/saturation, preserve exact profile identity/amount, exercise explicit captured profile/amount and the actual modal opt-in/save flow. The new restore regression fails before the fix (profile-preservation-before-fix.log) and passes after it. Native boundaries, unrelated drag/context menus and animation are mocked in unit tests; real hooks/stores/components/interpolation are used.

## Local preview and evidence

Private harness imports the actual final panels/hooks/stores/ProfileBrowser/Button/Text/Slider, actual generated application CSS and existing dark-theme variables/font stack. Synthetic camera/profile/preset metadata and native IPC/dialog/toast/context-menu boundaries are mocked. It is a component review fixture, not the full editor or an image render. Real framer-motion/DnD run in the browser preview. No new packages, account, photo or commercial profile content. System font fallback may apply when Poppins is unavailable.

- [Desktop capture](/private/tmp/rapidraw-ui1-20261015/desktop-cdp.png)
- [Phone-width capture with missing-base reason](/private/tmp/rapidraw-ui1-20261015/phone-cdp.png)
- [Phone-width wrong-destination guidance](/private/tmp/rapidraw-ui1-20261015/phone-import-error-cdp.png)

Final Chrome155 CDP metrics verify viewport390 and scrollWidth390 in both phone states; desktop scrollWidth1430≤1440. Images were captured after the final production fix and visually inspected. Earlier CLI phone.png was clipped and is superseded; capture startup-race log is retained. Native OS chooser interaction, touch/device behavior and production toast appearance were not exercised.

Preview server/browser are stopped; isolated browser caches/profiles were removed. To review again:

```sh
cd /Users/harrisontucker/.codex/worktrees/rapidraw-fuji-team-20261015/profile-preset-ui
node /private/tmp/rapidraw-ui1-20261015/make-preview.mjs
python3 -m http.server 18761 --bind 127.0.0.1 --directory /private/tmp/rapidraw-ui1-20261015/preview
```

Local URLs: http://127.0.0.1:18761/ (summary), /?open (browser/missing base), /?error (mock wrong-destination error). `capture-cdp.mjs` records all viewport/capture calls; start an isolated headless Chrome on loopback18762 using the recorded capture argv, then run it to reproduce three images. A viewport change is required for the actual390px phone-width check; the initial CLI window-size capture was insufficient.

Exact verification argv/exits/elapsed/log paths: *-result.json and verify.py. Node22.22.0, React/react-test-renderer19.2.8, esbuild0.28.2, TypeScript6.0.3, Prettier3.9.6, ESLint10.9.0; macOS26.5.2 arm64. Matching-lock node_modules was used read-only; no npm install or shared-cache writes. Vite runner avoids temporary config output in the shared node_modules; build artifacts are private. Initial mock resolver/animation-clock test failures and an intermediate TS required-property error are retained; final results above supersede them.

## Frozen paths/hashes

Patch (four existing production files + new actual-component test): `/private/tmp/rapidraw-ui1-20261015/source.patch`, SHA256 `513abab102cabbd560d9859b3fc2a261a8677b51e04eb7b15230e9bf920a5c9a`. Existing profileCompatibility.test.mjs and package-lock.json are unchanged. Only the five source/test paths below and this new receipt are modified; no other source edits. Browser non-label diffs are formatter reflows only.

| Owned changed path                            | SHA256                                                             |
| --------------------------------------------- | ------------------------------------------------------------------ |
| `src/components/panel/right/ProfilePanel.tsx` | `6562a2b534774949d0cf23ce958674c8b6a839183af1d467a4aa8a6cf0281034` |
| `src/components/panel/right/PresetsPanel.tsx` | `1e2b0b1dfedb9c06eca58d3babadb711e8d7fb25316af442b95faf27188fd7e0` |
| `src/components/ui/ProfileBrowser.tsx`        | `c99b61c9c2cbc3b108ee3502c799b9f745b1e82ba7ca9d8581d6a8dff499b5c1` |
| `src/i18n/locales/en.json`                    | `b75bf5e473007b8a94c96920542599c5f6ecbe9a66f1f6f8252f47e429e2d904` |
| `src/__tests__/profilePresetUi.test.mjs`      | `9067fc9cb47a4964cbd96bffc24af6c225b8b02bc13625eb4683c9ea8d6cb133` |

Private evidence `/private/tmp/rapidraw-ui1-20261015`: source-before/final-source-hashes, source.patch, baseline+final checks/logs, diagnostic comparison, preserved-semantics proof, before-fix regression and final screenshots. artifact-hashes.json freezes receipts/logs/preview/build artifacts; transient browser profiles were removed. This worker result is ready for independent review and does not authorize integration, a new app/APK build, installation or publication.

Design follows the assigned ownership brief and the existing editor, using [interface-design skill](/Users/harrisontucker/.codex/plugins/cache/claude-cowork/anthropic-skills/1.0.0/skills/interface-design/SKILL.md). Neutral panels, semantic theme tokens, existing Text/Button typography,4px spacing, visible focus and44px import/toolbar targets preserve the photo editor hierarchy. The visible base+Look foundation is repeated in the preset context/capture identity. No new palette, global navigation or design-system files.
