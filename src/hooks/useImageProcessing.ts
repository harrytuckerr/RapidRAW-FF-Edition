import React, { useCallback, useEffect, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import debounce from 'lodash.debounce';
import throttle from 'lodash.throttle';
import { toast } from 'react-toastify';
import { useEditorStore, type FinalPreviewProof } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { Adjustments, COPYABLE_ADJUSTMENT_KEYS, normalizeProfileBaseId } from '../utils/adjustments';
import { Invokes, Panel } from '../components/ui/AppProperties';
import { debouncedSave } from './useEditorActions';
import { globalImageCache } from '../utils/ImageLRUCache';
import { currentAccurateOpenKey, isCurrentOpenKey, sameOpenKey, sourceRenderPolicy } from '../utils/progressiveOpen';

type ApplySourceSnapshot = {
  path: string;
  openTicket: string | undefined;
  openKey: ReturnType<typeof currentAccurateOpenKey>;
  imageSnapshot: NonNullable<ReturnType<typeof useEditorStore.getState>['selectedImage']>;
};

type PendingApply = {
  adjustments: Adjustments;
  dragging: boolean;
  settings: ReturnType<typeof useSettingsStore.getState>['appSettings'];
  targetRes?: number;
  source: ApplySourceSnapshot;
  jobId: number;
  resolve: () => void;
  timing: ApplyPhaseMarks;
};

type ApplyPhaseMarks = {
  queuedAt: number;
  invokedAt?: number;
  respondedAt?: number;
  publishedAt?: number;
  nativeComplete?: boolean;
  failed?: boolean;
};

type PendingUncropped = {
  adjustments: Adjustments;
  source: ApplySourceSnapshot;
  queuedAt: number;
};

/**
 * Opt-in, bounded phase timing for real job IDs. Records carry literals and numbers only.
 * Store publication is not a paint callback, so no paint timing is claimed.
 */
export type PreviewPhaseTiming = {
  kind: 'uncropped' | 'apply';
  seq: number;
  phase: 'published' | 'native_complete' | 'discarded' | 'failed';
  queueWaitMs: number;
  invokeMs: number;
  publishMs: number | null;
};

const PREVIEW_TIMING_LIMIT = 64;
const previewPhaseTimings: PreviewPhaseTiming[] = [];

function previewTimingEnabled(): boolean {
  return (globalThis as { __RAPIDRAW_PREVIEW_TIMING__?: boolean }).__RAPIDRAW_PREVIEW_TIMING__ === true;
}

function recordPreviewTiming(record: PreviewPhaseTiming) {
  if (!previewTimingEnabled()) return;
  previewPhaseTimings.push(record);
  if (previewPhaseTimings.length > PREVIEW_TIMING_LIMIT) previewPhaseTimings.shift();
  console.debug('[preview-timing]', record);
}

export function getPreviewPhaseTimings(): readonly PreviewPhaseTiming[] {
  return previewPhaseTimings.slice();
}

async function decodePreviewUrl(url: string): Promise<void> {
  const image = new Image();
  image.src = url;
  await image.decode();
}

/** Tauri's custom-protocol path returns ArrayBuffer; its eval fallback can return JSON bytes. */
export function normalizePreviewResponse(value: unknown): ArrayBuffer {
  if (value instanceof ArrayBuffer) return value;
  if (value instanceof Uint8Array) return new Uint8Array(value).buffer;
  if (Array.isArray(value) && Array.from(value).every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) {
    return new Uint8Array(value).buffer;
  }
  throw new Error('Invalid preview response: expected binary image bytes.');
}

export function useImageProcessing(
  transformWrapperRef: any,
  prevAdjustmentsRef: React.RefObject<any>,
  renderRefs: {
    previewJobIdRef: React.RefObject<number>;
    latestRenderedJobIdRef: React.RefObject<number>;
    currentResRef: React.RefObject<number>;
  },
) {
  const { previewJobIdRef, latestRenderedJobIdRef, currentResRef } = renderRefs;

  const selectedImage = useEditorStore((state) => state.selectedImage);
  const adjustments = useEditorStore((state) => state.adjustments);
  const previewOverride = useEditorStore((state) => state.previewOverride);
  const isWaveformVisible = useEditorStore((state) => state.isWaveformVisible);
  const activeWaveformChannel = useEditorStore((state) => state.activeWaveformChannel);
  const displaySize = useEditorStore((state) => state.displaySize);
  const baseRenderSize = useEditorStore((state) => state.baseRenderSize);
  const originalSize = useEditorStore((state) => state.originalSize);
  const isSliderDragging = useEditorStore((state) => state.isSliderDragging);
  const setEditor = useEditorStore((state) => state.setEditor);

  const activeView = useUIStore((state) => state.activeView);
  const activePanel = useUIStore((state) => state.activePanel);
  const appSettings = useSettingsStore((state) => state.appSettings);
  const multiSelectedPaths = useLibraryStore((state) => state.multiSelectedPaths);

  const uncroppedJobIdRef = useRef(0);
  const latestUncroppedJobIdRef = useRef(0);
  const uncroppedInFlightRef = useRef(false);
  const uncroppedPendingRef = useRef<PendingUncropped | null>(null);
  const runUncroppedRef = useRef<(request: PendingUncropped) => void>(() => {});

  const inFlightCountRef = useRef(0);
  const lastAnalyticsTimeRef = useRef<number>(0);
  const pendingApplyRef = useRef<PendingApply | null>(null);
  const flushPipelineRef = useRef<() => void>(() => {});
  const mountedRef = useRef(true);
  const publishedProofRef = useRef<FinalPreviewProof | null>(null);
  const dragIdleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeWaveformChannelRef = useRef(activeWaveformChannel);
  activeWaveformChannelRef.current = activeWaveformChannel;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      const proof = publishedProofRef.current;
      if (proof && useEditorStore.getState().finalPreviewProof === proof)
        useEditorStore.getState().setEditor({ finalPreviewProof: null });
      publishedProofRef.current = null;
      pendingApplyRef.current?.resolve();
      pendingApplyRef.current = null;
    };
  }, []);

  const calculateROI = useCallback(() => {
    if (!transformWrapperRef.current) return null;
    const state = transformWrapperRef.current.instance.transformState;
    if (!state) return null;

    if (!baseRenderSize) return null;

    const { scale, positionX, positionY } = state;
    const { width: baseW, height: baseH, offsetX, offsetY, containerWidth, containerHeight } = baseRenderSize;

    if (!baseW || !baseH || !containerWidth || !containerHeight) return null;
    if (scale <= 1.01) return null;

    const paddingPixels = 2.0;
    const paddingX = paddingPixels / baseW;
    const paddingY = paddingPixels / baseH;

    const visibleLeft = -positionX / scale;
    const visibleTop = -positionY / scale;
    const visibleRight = visibleLeft + containerWidth / scale;
    const visibleBottom = visibleTop + containerHeight / scale;

    const imgLeft = offsetX;
    const imgTop = offsetY;
    const imgRight = offsetX + baseW;
    const imgBottom = offsetY + baseH;

    const intersectLeft = Math.max(visibleLeft, imgLeft);
    const intersectTop = Math.max(visibleTop, imgTop);
    const intersectRight = Math.min(visibleRight, imgRight);
    const intersectBottom = Math.min(visibleBottom, imgBottom);

    if (intersectLeft >= intersectRight || intersectTop >= intersectBottom) {
      return null;
    }

    const roiX = (intersectLeft - imgLeft) / baseW;
    const roiY = (intersectTop - imgTop) / baseH;
    const roiW = (intersectRight - intersectLeft) / baseW;
    const roiH = (intersectBottom - intersectTop) / baseH;

    const newRoiX = roiX - paddingX;
    const newRoiY = roiY - paddingY;
    const newRoiW = roiW + paddingX * 2;
    const newRoiH = roiH + paddingY * 2;

    const clampedX = Math.max(0, newRoiX);
    const clampedY = Math.max(0, newRoiY);
    const clampedW = Math.min(1 - clampedX, newRoiW);
    const clampedH = Math.min(1 - clampedY, newRoiH);

    if (clampedW > 0.999 && clampedH > 0.999) return null;

    return [clampedX, clampedY, clampedW, clampedH] as [number, number, number, number];
  }, [baseRenderSize, transformWrapperRef]);

  const isCurrentApplySource = useCallback((source: ApplySourceSnapshot) => {
    const state = useEditorStore.getState();
    const image = state.selectedImage;
    if (
      !mountedRef.current ||
      state.pendingOpenIntent ||
      !image?.isReady ||
      image.path !== source.path ||
      image.openTicket !== source.openTicket
    )
      return false;

    if (source.openKey) {
      return sameOpenKey(image.openKey, source.openKey) && isCurrentOpenKey(source.openKey, state, source.path, true);
    }

    // Legacy sources have no wire key, so retain the exact selected-image object as their lifetime token.
    return !image.openTicket && !image.openKey && image === source.imageSnapshot;
  }, []);

  const captureApplySource = useCallback((): ApplySourceSnapshot | null => {
    const state = useEditorStore.getState();
    const image = state.selectedImage;
    if (
      !mountedRef.current ||
      state.pendingOpenIntent ||
      !selectedImage?.isReady ||
      !image?.isReady ||
      image !== selectedImage
    )
      return null;

    const openKey = currentAccurateOpenKey(state);
    if (
      (image.openTicket && (!openKey || !sameOpenKey(image.openKey, openKey))) ||
      (!image.openTicket && image.openKey)
    )
      return null;

    return {
      path: image.path,
      openTicket: image.openTicket,
      openKey,
      imageSnapshot: image,
    };
  }, [selectedImage]);
  // The memoized uncropped throttle must sample the current render's source, not the first render's.
  const captureApplySourceRef = useRef(captureApplySource);
  captureApplySourceRef.current = captureApplySource;

  const performApplyAdjustments = useCallback(
    async (request: PendingApply) => {
      const { adjustments: currentAdjustments, dragging, targetRes, source, jobId } = request;
      const { path: currentPath, openKey } = source;
      if (!isCurrentApplySource(source)) return;
      const stillCurrent = () => isCurrentApplySource(source);

      let shouldRequestAnalytics = false;
      if (dragging) {
        const now = performance.now();
        if (now - lastAnalyticsTimeRef.current > 33.33) {
          shouldRequestAnalytics = true;
          lastAnalyticsTimeRef.current = now;
        }
      } else {
        shouldRequestAnalytics = true;
        lastAnalyticsTimeRef.current = 0;
      }

      const payload = structuredClone(currentAdjustments);
      const { patchesSentToBackend } = useEditorStore.getState();
      const newlySentPatches = new Set<string>();

      const processSubMasks = (subMasks: any[]) => {
        // Keyed requests own their inline data; the legacy ID cache is not source authority.
        if (openKey || !Array.isArray(subMasks)) return;
        subMasks.forEach((sm: any) => {
          if (sm.id && sm.parameters) {
            const keys = ['mask_data_base64', 'maskDataBase64'];
            let foundMaskData = false;

            for (const key of keys) {
              if (sm.parameters[key] !== undefined && sm.parameters[key] !== null) {
                foundMaskData = true;
                if (patchesSentToBackend.has(sm.id)) {
                  sm.parameters[key] = null;
                }
              }
            }
            if (foundMaskData && !patchesSentToBackend.has(sm.id)) {
              newlySentPatches.add(sm.id);
            }
          }
        });
      };

      if (payload.aiPatches && Array.isArray(payload.aiPatches)) {
        payload.aiPatches.forEach((p: any) => {
          if (!openKey && p.id && p.patchData && !p.isLoading) {
            if (patchesSentToBackend.has(p.id)) {
              p.patchData = null;
            } else {
              newlySentPatches.add(p.id);
            }
          }
          if (p.subMasks) processSubMasks(p.subMasks);
        });
      }

      if (payload.masks && Array.isArray(payload.masks)) {
        payload.masks.forEach((container: any) => {
          if (container.subMasks) processSubMasks(container.subMasks);
        });
      }

      const roi = calculateROI();

      try {
        if (payload.profile != null) {
          if (typeof payload.profile !== 'object' || Array.isArray(payload.profile)) {
            throw new Error('Invalid profile: expected an object.');
          }
          payload.profile.base = normalizeProfileBaseId(payload.profile.base);
        }
        request.timing.invokedAt = performance.now();
        let response: unknown;
        try {
          response = await invoke(Invokes.ApplyAdjustments, {
            jsAdjustments: payload,
            isInteractive: dragging,
            targetResolution: targetRes || null,
            roi: roi || null,
            requestAnalytics: shouldRequestAnalytics,
            computeWaveform: !!isWaveformVisible,
            activeWaveformChannel: activeWaveformChannelRef.current || null,
            ...(openKey ? { expectedOpenKey: openKey } : {}),
          });
        } finally {
          // Settlement time is recorded for rejections too, so invokeMs is a real duration.
          request.timing.respondedAt = performance.now();
        }

        if (!stillCurrent()) return;
        if (newlySentPatches.size > 0) newlySentPatches.forEach((id) => patchesSentToBackend.add(id));

        const buffer = normalizePreviewResponse(response);
        if (buffer.byteLength === 0) throw new Error('Preview renderer returned no image bytes.');

        if (buffer.byteLength > 0 && jobId >= latestRenderedJobIdRef.current) {
          if (jobId === previewJobIdRef.current) toast.dismiss('preview-render-error');
          latestRenderedJobIdRef.current = jobId;

          const textDecoder = new TextDecoder();
          const prefix = textDecoder.decode(buffer.slice(0, 11));
          if (prefix === 'WGPU_RENDER') {
            if (sourceRenderPolicy(useEditorStore.getState()) !== 'legacy-native') {
              throw new Error('The native renderer returned a frame for a source that requires encoded CPU display.');
            }
            // A native sentinel is a successful native completion, not a store frame publication.
            request.timing.nativeComplete = true;
            setEditor((state) => {
              if (state.interactivePatch && state.interactivePatch.url) URL.revokeObjectURL(state.interactivePatch.url);
              return { interactivePatch: null, finalPreviewProof: null };
            });
            return;
          }

          if (dragging) {
            const view = new DataView(buffer);
            const patchX = view.getUint32(0, true);
            const patchY = view.getUint32(4, true);
            const patchW = view.getUint32(8, true);
            const patchH = view.getUint32(12, true);
            const fullW = view.getUint32(16, true);
            const fullH = view.getUint32(20, true);

            const imageBuffer = buffer.slice(24);
            const blob = new Blob([imageBuffer], { type: 'image/jpeg' });
            const url = URL.createObjectURL(blob);

            try {
              if (openKey) await decodePreviewUrl(url);
            } catch {
              URL.revokeObjectURL(url);
              throw new Error('Preview image could not be decoded.');
            }
            if (!stillCurrent() || jobId < latestRenderedJobIdRef.current) {
              URL.revokeObjectURL(url);
              return;
            }

            request.timing.publishedAt = performance.now();
            setEditor((state) => {
              if (state.interactivePatch && state.interactivePatch.url)
                setTimeout(() => URL.revokeObjectURL(state.interactivePatch.url), 100);
              return {
                interactivePatch: {
                  url,
                  normX: patchX / fullW,
                  normY: patchY / fullH,
                  normW: patchW / fullW,
                  normH: patchH / fullH,
                },
              };
            });
          } else {
            const blob = new Blob([buffer], { type: 'image/jpeg' });
            const url = URL.createObjectURL(blob);

            try {
              if (openKey) await decodePreviewUrl(url);
            } catch {
              URL.revokeObjectURL(url);
              throw new Error('Preview image could not be decoded.');
            }

            if (!stillCurrent() || jobId < latestRenderedJobIdRef.current) {
              URL.revokeObjectURL(url);
              return;
            }

            request.timing.publishedAt = performance.now();
            setEditor((state) => {
              const prevUrl = state.finalPreviewUrl;
              if (prevUrl && prevUrl.startsWith('blob:') && !globalImageCache.isProtected(prevUrl)) {
                setTimeout(() => {
                  if (!globalImageCache.isProtected(prevUrl)) {
                    URL.revokeObjectURL(prevUrl);
                  }
                }, 250);
              }
              const isCurrent = () => {
                const current = useEditorStore.getState();
                return (
                  mountedRef.current &&
                  useUIStore.getState().activeView === 'editor' &&
                  jobId === previewJobIdRef.current &&
                  stillCurrent() &&
                  current.selectedImage === source.imageSnapshot &&
                  current.adjustments === currentAdjustments &&
                  useSettingsStore.getState().appSettings === request.settings &&
                  !current.previewOverride &&
                  !current.showOriginal
                );
              };
              const proof: FinalPreviewProof | null =
                !dragging && isCurrent()
                  ? {
                      url,
                      source: source.imageSnapshot,
                      openKey: openKey ?? null,
                      adjustments: currentAdjustments,
                      settings: request.settings,
                      jobId,
                      isCurrent,
                    }
                  : null;
              publishedProofRef.current = proof;
              return { finalPreviewUrl: url, finalPreviewProof: proof };
            });

            setEditor((state) => {
              if (state.interactivePatch && state.interactivePatch.url) {
                setTimeout(() => URL.revokeObjectURL(state.interactivePatch.url), 500);
              }
              return { interactivePatch: null };
            });
          }
        }
      } catch (err) {
        if (!stillCurrent() || jobId !== previewJobIdRef.current) return;
        if (err === 'Preview superseded' || err === 'Superseded or worker failed') return;
        request.timing.failed = true;
        console.error('Failed to apply adjustments:', err);
        toast.error(err instanceof Error ? err.message : String(err), { toastId: 'preview-render-error' });
        if (!dragging) {
          setEditor((state) => {
            if (state.interactivePatch && state.interactivePatch.url) URL.revokeObjectURL(state.interactivePatch.url);
            return { interactivePatch: null };
          });
        }
      }
    },
    [calculateROI, isCurrentApplySource, isWaveformVisible, setEditor, previewJobIdRef, latestRenderedJobIdRef],
  );

  const flushPipeline = useCallback(() => {
    if (inFlightCountRef.current >= 1 || !pendingApplyRef.current) return;

    const request = pendingApplyRef.current;
    pendingApplyRef.current = null;
    if (!isCurrentApplySource(request.source)) {
      request.resolve();
      return;
    }

    inFlightCountRef.current += 1;
    void performApplyAdjustments(request)
      .catch((err) => {
        if (!isCurrentApplySource(request.source) || request.jobId !== previewJobIdRef.current) return;
        request.timing.failed = true;
        console.error('Failed to apply adjustments:', err);
        toast.error(err instanceof Error ? err.message : String(err), { toastId: 'preview-render-error' });
        if (!request.dragging) {
          setEditor((state) => {
            if (state.interactivePatch && state.interactivePatch.url) URL.revokeObjectURL(state.interactivePatch.url);
            return { interactivePatch: null };
          });
        }
      })
      .finally(() => {
        const marks = request.timing;
        const invokedAt = marks.invokedAt ?? marks.queuedAt;
        const respondedAt = marks.respondedAt ?? invokedAt;
        recordPreviewTiming({
          kind: 'apply',
          seq: request.jobId,
          phase:
            marks.publishedAt !== undefined
              ? 'published'
              : marks.failed
                ? 'failed'
                : marks.nativeComplete
                  ? 'native_complete'
                  : 'discarded',
          queueWaitMs: Math.round(invokedAt - marks.queuedAt),
          invokeMs: Math.round(respondedAt - invokedAt),
          publishMs: marks.publishedAt !== undefined ? Math.round(marks.publishedAt - marks.queuedAt) : null,
        });
        inFlightCountRef.current -= 1;
        request.resolve();
        if (pendingApplyRef.current) flushPipelineRef.current();
      });
  }, [isCurrentApplySource, performApplyAdjustments, previewJobIdRef, setEditor]);
  flushPipelineRef.current = flushPipeline;

  const executeApplyAdjustments = useCallback(
    (currentAdjustments: Adjustments, dragging: boolean = false, targetRes?: number) => {
      const source = captureApplySource();
      if (!source) return Promise.resolve();
      // Admission invalidates the old proof even while the old Blob remains displayed.
      useEditorStore.getState().setEditor({ finalPreviewProof: null });
      publishedProofRef.current = null;

      return new Promise<void>((resolve) => {
        const superseded = pendingApplyRef.current;
        pendingApplyRef.current = {
          adjustments: currentAdjustments,
          dragging,
          settings: useSettingsStore.getState().appSettings,
          targetRes,
          source,
          jobId: ++previewJobIdRef.current,
          resolve,
          timing: { queuedAt: performance.now() },
        };
        superseded?.resolve();
        flushPipelineRef.current();
      });
    },
    [captureApplySource, previewJobIdRef],
  );

  const applyAdjustments = useCallback(
    (currentAdjustments: Adjustments, dragging: boolean = false, targetRes?: number) => {
      void executeApplyAdjustments(currentAdjustments, dragging, targetRes);
    },
    [executeApplyAdjustments],
  );

  const runUncroppedPreview = useCallback(
    (request: PendingUncropped) => {
      // The request carries the source captured at admission. It is never rebuilt from current state.
      const source = request.source;
      if (!isCurrentApplySource(source)) return;
      const jobId = ++uncroppedJobIdRef.current;
      const invokedAt = performance.now();
      uncroppedInFlightRef.current = true;
      invoke<string>(Invokes.GenerateUncroppedPreview, {
        jsAdjustments: request.adjustments,
        ...(source.openKey ? { expectedOpenKey: source.openKey } : {}),
      })
        .then((dataUrl) => {
          const respondedAt = performance.now();
          const publish = isCurrentApplySource(source) && jobId >= latestUncroppedJobIdRef.current;
          if (publish) {
            latestUncroppedJobIdRef.current = jobId;
            useEditorStore.getState().setEditor({ uncroppedAdjustedPreviewUrl: dataUrl });
          }
          recordPreviewTiming({
            kind: 'uncropped',
            seq: jobId,
            phase: publish ? 'published' : 'discarded',
            queueWaitMs: Math.round(invokedAt - request.queuedAt),
            invokeMs: Math.round(respondedAt - invokedAt),
            publishMs: publish ? Math.round(performance.now() - request.queuedAt) : null,
          });
        })
        .catch((error) => {
          console.error(error);
          recordPreviewTiming({
            kind: 'uncropped',
            seq: jobId,
            phase: 'failed',
            queueWaitMs: Math.round(invokedAt - request.queuedAt),
            invokeMs: Math.round(performance.now() - invokedAt),
            publishMs: null,
          });
        })
        .finally(() => {
          uncroppedInFlightRef.current = false;
          const next = uncroppedPendingRef.current;
          uncroppedPendingRef.current = null;
          if (next && mountedRef.current) runUncroppedRef.current(next);
        });
    },
    [isCurrentApplySource],
  );
  runUncroppedRef.current = runUncroppedPreview;

  // One active request plus the latest pending request. Admission and every later step
  // validate the source captured before the throttle, so stale work never reaches dispatch.
  const throttledUncroppedPreview = useMemo(
    () =>
      throttle(
        (request: PendingUncropped) => {
          if (!isCurrentApplySource(request.source)) return;
          if (uncroppedInFlightRef.current) {
            uncroppedPendingRef.current = request;
            return;
          }
          runUncroppedRef.current(request);
        },
        30,
        { leading: true, trailing: true },
      ),
    [isCurrentApplySource],
  );

  useEffect(
    () => () => {
      throttledUncroppedPreview.cancel();
      uncroppedPendingRef.current = null;
    },
    [throttledUncroppedPreview],
  );

  const generateUncroppedPreview = useCallback(
    (currentAdjustments: Adjustments) => {
      // Capture the source before throttle admission; the throttle keeps this exact request.
      const source = captureApplySourceRef.current();
      if (!source) return;
      throttledUncroppedPreview({ adjustments: currentAdjustments, source, queuedAt: performance.now() });
    },
    [throttledUncroppedPreview],
  );

  useEffect(() => {
    if (activeView === 'editor' && activePanel === Panel.Crop && selectedImage?.isReady) {
      generateUncroppedPreview(adjustments);
    }
  }, [activeView, adjustments, activePanel, selectedImage?.isReady, generateUncroppedPreview]);

  const calculateTargetRes = useCallback(() => {
    const baseTargetRes = appSettings?.editorPreviewResolution || 1920;
    if (!(appSettings?.enableZoomHifi ?? true) || displaySize.width === 0) {
      return baseTargetRes;
    }

    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1;
    const sharpnessFactor = 1.25;
    const zoomMultiplier = appSettings?.highResZoomMultiplier || 1.0;
    const effectiveDpr = appSettings?.useFullDpiRendering ? dpr : 1;

    let targetRes = Math.max(displaySize.width, displaySize.height) * effectiveDpr * sharpnessFactor * zoomMultiplier;
    targetRes = Math.max(targetRes, 512);

    if (originalSize && originalSize.width > 0 && originalSize.height > 0) {
      const origMax = Math.max(originalSize.width, originalSize.height);
      targetRes = Math.min(targetRes, origMax);
      if (targetRes >= origMax * 0.8) {
        targetRes = origMax;
      }
    }

    if (originalSize && targetRes !== Math.max(originalSize.width, originalSize.height)) {
      targetRes = Math.ceil(targetRes / 256) * 256;
    }

    return Math.round(targetRes);
  }, [
    appSettings?.enableZoomHifi,
    appSettings?.editorPreviewResolution,
    appSettings?.highResZoomMultiplier,
    appSettings?.useFullDpiRendering,
    displaySize.width,
    displaySize.height,
    originalSize,
  ]);

  const requestHiFiZoom = useMemo(
    () =>
      debounce((targetRes: number) => {
        if (targetRes > currentResRef.current) {
          currentResRef.current = targetRes;
          const { adjustments, previewOverride } = useEditorStore.getState();
          const renderAdjustments = previewOverride ?? adjustments;
          applyAdjustments(renderAdjustments, false, targetRes);
        }
      }, 50),
    [applyAdjustments, currentResRef],
  );

  useEffect(() => {
    if (activeView === 'editor' && selectedImage?.isReady && displaySize.width > 0 && !isSliderDragging) {
      let baseRes = calculateTargetRes();
      if (originalSize.width > 0 && originalSize.height > 0) {
        const maxRes = Math.max(originalSize.width, originalSize.height);
        if (baseRes > maxRes) baseRes = maxRes;
      }
      const finalRes = Math.round(baseRes);

      if (finalRes > currentResRef.current) {
        requestHiFiZoom(finalRes);
      }
    }
    return () => {
      requestHiFiZoom.cancel();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    activeView,
    displaySize.width,
    displaySize.height,
    calculateTargetRes,
    selectedImage?.isReady,
    isSliderDragging,
    requestHiFiZoom,
    originalSize,
  ]);

  useEffect(() => {
    if (!selectedImage?.isReady) return;

    if (dragIdleTimer.current) clearTimeout(dragIdleTimer.current);

    const targetRes = calculateTargetRes();
    const renderAdjustments = previewOverride ?? adjustments;

    if (activeView !== 'editor') {
      if (isSliderDragging) return;
    }

    if (isSliderDragging) {
      if (appSettings?.enableLivePreviews !== false) {
        applyAdjustments(renderAdjustments, true, targetRes);
      }
    } else {
      dragIdleTimer.current = setTimeout(() => {
        currentResRef.current = targetRes;

        applyAdjustments(renderAdjustments, false, targetRes);

        if (previewOverride) return;

        const prev = prevAdjustmentsRef.current;

        if (!prev || prev.path !== selectedImage.path) {
          prevAdjustmentsRef.current = { path: selectedImage.path, adjustments };
          return;
        }

        const hasAdjustmentsChanged = prev.adjustments !== adjustments;

        if (hasAdjustmentsChanged) {
          debouncedSave(selectedImage.path, adjustments);

          const otherPaths = multiSelectedPaths.filter((p) => p !== selectedImage.path);
          if (appSettings?.copyPasteSettings?.autoSync && otherPaths.length > 0) {
            const delta: Partial<Adjustments> = {};
            const includedKeys = appSettings?.copyPasteSettings?.includedAdjustments || COPYABLE_ADJUSTMENT_KEYS;
            for (const key of Object.keys(adjustments) as Array<keyof Adjustments>) {
              if (includedKeys.includes(key as string)) {
                if (JSON.stringify(adjustments[key]) !== JSON.stringify(prev.adjustments[key])) {
                  (delta as any)[key] = adjustments[key];
                }
              }
            }
            if (Object.keys(delta).length > 0) {
              otherPaths.forEach((p) => globalImageCache.delete(p));
              invoke(Invokes.ApplyAdjustmentsToPaths, { paths: otherPaths, adjustments: delta }).catch((err) => {
                console.error('Failed to apply adjustments to multi-selection:', err);
              });
            }
          }

          prevAdjustmentsRef.current = { path: selectedImage.path, adjustments };
        }
      }, 50);
    }

    return () => {
      if (dragIdleTimer.current) clearTimeout(dragIdleTimer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    activeView,
    adjustments,
    previewOverride,
    selectedImage?.path,
    selectedImage?.isReady,
    isSliderDragging,
    multiSelectedPaths,
    appSettings?.enableLivePreviews,
    appSettings?.enableBaselineExposure,
    appSettings?.baselineExposure,
    appSettings?.compensateHighlightPreservation,
    appSettings?.copyPasteSettings?.includedAdjustments,
    appSettings?.copyPasteSettings?.autoSync,
    isWaveformVisible,
  ]);

  return {
    applyAdjustments,
    executeApplyAdjustments,
  };
}
