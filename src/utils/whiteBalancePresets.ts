import {
  MAX_TEMPERATURE,
  MAX_TINT,
  MIN_TEMPERATURE,
  withKelvinWhiteBalance,
  withRelativeWhiteBalance,
} from './whiteBalance';
import type { WhiteBalance } from './whiteBalance';

export const WHITE_BALANCE_PRESETS = [
  { id: 'daylight', temperature: 5500 },
  { id: 'shade', temperature: 7500 },
  { id: 'cloudy', temperature: 6500 },
  { id: 'tungsten', temperature: 3200 },
  { id: 'fluorescent', temperature: 4000 },
  { id: 'flash', temperature: 5500 },
] as const;

export const applyWhiteBalancePreset = <
  T extends { temperature: number; tint: number; whiteBalance?: WhiteBalance | null },
>(
  adjustments: T,
  presetId: string,
): T => {
  if (presetId === 'asShot') return withRelativeWhiteBalance(adjustments, { temperature: 0, tint: 0 });
  const preset = WHITE_BALANCE_PRESETS.find(({ id }) => id === presetId);
  return preset ? withKelvinWhiteBalance(adjustments, { temperature: preset.temperature, tint: 0 }) : adjustments;
};

export interface WhiteBalanceSample extends WhiteBalance {
  r: number;
  g: number;
  b: number;
  count: number;
}

export interface WhiteBalanceRequestFence {
  currentImageSource: object | null;
  currentImageSourceGeneration: number;
  currentRequestGeneration: number;
  currentWhiteBalance: { temperature: number; tint: number; whiteBalance?: WhiteBalance | null };
  currentWhiteBalanceRevision: number;
  imageSource: object;
  imageSourceGeneration: number;
  requestGeneration: number;
  whiteBalance: { temperature: number; tint: number; whiteBalance?: WhiteBalance | null };
  whiteBalanceRevision: number;
}

export interface WhiteBalanceControls {
  temperature: number;
  tint: number;
  whiteBalance?: WhiteBalance | null;
}

export const sameWhiteBalance = (left: WhiteBalance | null | undefined, right: WhiteBalance | null | undefined) =>
  left === right || (!!left && !!right && left.temperature === right.temperature && left.tint === right.tint);

export const sameWhiteBalanceControls = (left: WhiteBalanceControls, right: WhiteBalanceControls) =>
  left.temperature === right.temperature &&
  left.tint === right.tint &&
  sameWhiteBalance(left.whiteBalance, right.whiteBalance);

export const trackWhiteBalanceRequestRevisions = <TState>(
  subscribe: (listener: (state: TState, previousState: TState) => void) => () => void,
  getImageSource: (state: TState) => object | null,
  getWhiteBalance: (state: TState) => WhiteBalanceControls,
) => {
  let imageSourceGeneration = 0;
  let whiteBalanceRevision = 0;
  const unsubscribe = subscribe((state, previousState) => {
    if (getImageSource(state) !== getImageSource(previousState)) imageSourceGeneration += 1;
    if (!sameWhiteBalanceControls(getWhiteBalance(state), getWhiteBalance(previousState))) whiteBalanceRevision += 1;
  });
  return {
    getRevisions: () => ({ imageSourceGeneration, whiteBalanceRevision }),
    unsubscribe,
  };
};

export const canApplyWhiteBalanceRequest = (fence: WhiteBalanceRequestFence) =>
  fence.requestGeneration === fence.currentRequestGeneration &&
  fence.imageSourceGeneration === fence.currentImageSourceGeneration &&
  fence.imageSource === fence.currentImageSource &&
  fence.whiteBalanceRevision === fence.currentWhiteBalanceRevision &&
  fence.whiteBalance.temperature === fence.currentWhiteBalance.temperature &&
  fence.whiteBalance.tint === fence.currentWhiteBalance.tint &&
  sameWhiteBalance(fence.whiteBalance.whiteBalance, fence.currentWhiteBalance.whiteBalance);

export const isUsableWhiteBalanceSample = (
  sample: WhiteBalanceSample | null | undefined,
): sample is WhiteBalanceSample =>
  !!sample &&
  Number.isFinite(sample.r) &&
  Number.isFinite(sample.g) &&
  Number.isFinite(sample.b) &&
  sample.r > 0 &&
  sample.g > 0 &&
  sample.b > 0 &&
  Number.isFinite(sample.count) &&
  Number.isInteger(sample.count) &&
  sample.count > 0 &&
  Number.isFinite(sample.temperature) &&
  sample.temperature >= MIN_TEMPERATURE &&
  sample.temperature <= MAX_TEMPERATURE &&
  Number.isFinite(sample.tint) &&
  Math.abs(sample.tint) <= MAX_TINT;
