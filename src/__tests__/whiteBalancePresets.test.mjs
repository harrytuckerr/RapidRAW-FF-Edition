import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { build } from 'esbuild';

const require = createRequire(import.meta.url);
const bundle = await build({
  stdin: {
    contents: `export { WHITE_BALANCE_PRESETS, applyWhiteBalancePreset, canApplyWhiteBalanceRequest, isUsableWhiteBalanceSample, trackWhiteBalanceRequestRevisions } from './src/utils/whiteBalancePresets';`,
    resolveDir: process.cwd(),
  },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
});
const compiled = { exports: {} };
new Function('require', 'module', 'exports', bundle.outputFiles[0].text)(require, compiled, compiled.exports);
const {
  WHITE_BALANCE_PRESETS,
  applyWhiteBalancePreset,
  canApplyWhiteBalanceRequest,
  isUsableWhiteBalanceSample,
  trackWhiteBalanceRequestRevisions,
} = compiled.exports;

const sample = { r: 0.3, g: 0.3, b: 0.3, count: 120, temperature: 5500, tint: 0 };

test('nominal Kelvin menu values remain editing presets and keep Daylight/Flash identity ambiguous', () => {
  assert.deepEqual(
    WHITE_BALANCE_PRESETS.map(({ id, temperature }) => [id, temperature]),
    [
      ['daylight', 5500],
      ['shade', 7500],
      ['cloudy', 6500],
      ['tungsten', 3200],
      ['fluorescent', 4000],
      ['flash', 5500],
    ],
  );
});

test('preset application changes only WB controls and As Shot clears the absolute override', () => {
  const adjustments = {
    exposure: 1.25,
    profile: { id: 'camera-base' },
    temperature: 12,
    tint: -4,
    whiteBalance: { temperature: 6100, tint: 7 },
  };
  const shade = applyWhiteBalancePreset(adjustments, 'shade');
  assert.deepEqual(shade, {
    ...adjustments,
    temperature: 0,
    tint: 0,
    whiteBalance: { temperature: 7500, tint: 0 },
  });
  const asShot = applyWhiteBalancePreset(shade, 'asShot');
  assert.deepEqual(asShot, { ...shade, temperature: 0, tint: 0, whiteBalance: null });
});

test('Auto accepts finite non-black full-frame samples within supported bounds', () => {
  assert.equal(isUsableWhiteBalanceSample(sample), true);
  assert.equal(isUsableWhiteBalanceSample({ ...sample, count: 0 }), false);
  assert.equal(isUsableWhiteBalanceSample({ ...sample, count: 1.5 }), false);
  assert.equal(isUsableWhiteBalanceSample({ ...sample, r: 0, g: 0, b: 0 }), false);
  for (const channel of ['r', 'g', 'b']) {
    for (const value of [0, -0.01]) {
      assert.equal(isUsableWhiteBalanceSample({ ...sample, [channel]: value }), false);
    }
  }
  assert.equal(isUsableWhiteBalanceSample({ ...sample, temperature: Number.NaN }), false);
  assert.equal(isUsableWhiteBalanceSample({ ...sample, temperature: 50001 }), false);
  assert.equal(isUsableWhiteBalanceSample({ ...sample, tint: 151 }), false);
});

test('Auto result fence rejects newer requests, source changes, and manual WB edits', () => {
  const imageA = {};
  const base = {
    currentImageSource: imageA,
    currentImageSourceGeneration: 3,
    currentRequestGeneration: 7,
    currentWhiteBalance: { temperature: 0, tint: 0, whiteBalance: null },
    currentWhiteBalanceRevision: 2,
    imageSource: imageA,
    imageSourceGeneration: 3,
    requestGeneration: 7,
    whiteBalance: { temperature: 0, tint: 0, whiteBalance: null },
    whiteBalanceRevision: 2,
  };
  assert.equal(canApplyWhiteBalanceRequest(base), true);
  assert.equal(canApplyWhiteBalanceRequest({ ...base, currentRequestGeneration: 8 }), false);
  assert.equal(canApplyWhiteBalanceRequest({ ...base, currentImageSourceGeneration: 5 }), false);
  assert.equal(
    canApplyWhiteBalanceRequest({
      ...base,
      currentWhiteBalanceRevision: 3,
      currentWhiteBalance: { temperature: 15, tint: 0, whiteBalance: null },
    }),
    false,
  );
});

test('request revisions observe direct store WB edits and A-to-B-to-A image selections', () => {
  const imageA = {};
  const imageB = {};
  let state = { source: imageA, adjustments: { temperature: 0, tint: 0, whiteBalance: null } };
  let listener;
  const tracker = trackWhiteBalanceRequestRevisions(
    (subscribe) => {
      listener = subscribe;
      return () => {
        listener = undefined;
      };
    },
    (current) => current.source,
    (current) => current.adjustments,
  );
  const updateStoreDirectly = (next) => {
    const previous = state;
    state = next;
    listener(state, previous);
  };

  updateStoreDirectly({ ...state, adjustments: { ...state.adjustments, temperature: 20 } });
  updateStoreDirectly({ ...state, adjustments: { ...state.adjustments, temperature: 0 } });
  updateStoreDirectly({ ...state, source: imageB });
  updateStoreDirectly({ ...state, source: imageA });
  assert.deepEqual(tracker.getRevisions(), { imageSourceGeneration: 2, whiteBalanceRevision: 2 });
  tracker.unsubscribe();
});
