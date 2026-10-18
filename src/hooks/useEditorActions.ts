import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import debounce from 'lodash.debounce';
import { toast } from 'react-toastify';
import { useEditorStore, type FinalPreviewProof } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { useProcessStore } from '../store/useProcessStore';
import {
  Adjustments,
  INITIAL_ADJUSTMENTS,
  COPYABLE_ADJUSTMENT_KEYS,
  PasteMode,
  LensAdjustment,
  normalizeLoadedAdjustments,
} from '../utils/adjustments';
import { calculateAutoCropForRotation, calculateCenteredCrop } from '../utils/cropUtils';
import { Invokes } from '../components/ui/AppProperties';
import { globalImageCache } from '../utils/ImageLRUCache';
import { isProgressiveMutationBlocked } from '../utils/progressiveOpen';
import { currentAccurateOpenKey, isCurrentOpenKey, sourceRenderPolicy } from '../utils/progressiveOpen';

// Lens transactions use the current source/adjustment identities; no parallel store revision model.
export interface LensSelectionIntent {
  path: string;
  source: NonNullable<ReturnType<typeof useEditorStore.getState>['selectedImage']>;
  adjustments: Adjustments;
  isCurrent(): boolean;
}

export const debouncedSetHistory = debounce((newAdj: Adjustments, lensSource?: LensSelectionIntent['source']) => {
  const current = useEditorStore.getState();
  if (
    lensSource &&
    (current.selectedImage !== lensSource ||
      current.adjustments !== newAdj ||
      isProgressiveMutationBlocked(current) ||
      sourceRenderPolicy(current) === 'locked')
  )
    return;
  current.pushHistory(newAdj);
}, 500);

const activeSavePromises = new Set<Promise<boolean>>();
const failedSavePaths = new Set<string>();

export const debouncedSave = debounce((path: string, adjustmentsToSave: Adjustments) => {
  const save = invoke(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: adjustmentsToSave })
    .then(() => {
      failedSavePaths.delete(path);
      return true;
    })
    .catch((err) => {
      failedSavePaths.add(path);
      console.error('Auto-save failed:', err);
      toast.error(`Failed to save changes: ${err}`);
      return false;
    });
  activeSavePromises.add(save);
  void save.finally(() => activeSavePromises.delete(save));
  return save;
}, 300);

export async function flushEditorSavesBeforeOpen(): Promise<boolean> {
  const flushed = debouncedSave.flush();
  const launched =
    flushed && typeof (flushed as Promise<boolean>).then === 'function' ? [flushed as Promise<boolean>] : [];
  const results = await Promise.all([...activeSavePromises, ...launched]);
  return results.every(Boolean) && failedSavePaths.size === 0;
}

function filterProgressiveMutationTargets(
  paths: string[],
  state: ReturnType<typeof useEditorStore.getState>,
): string[] {
  const blocked = new Set<string>();
  if (state.pendingOpenIntent?.path) blocked.add(state.pendingOpenIntent.path);
  const selectedImage = state.selectedImage;
  const currentCanMutate = Boolean(
    selectedImage?.isReady &&
    !isProgressiveMutationBlocked(state) &&
    (!selectedImage.openTicket || currentAccurateOpenKey(state)),
  );
  if (selectedImage && !currentCanMutate) blocked.add(selectedImage.path);
  return paths.filter((path) => !blocked.has(path));
}

export function useEditorActions() {
  const setEditor = useEditorStore((s) => s.setEditor);

  const setAdjustments = useCallback(
    (
      value: Partial<Adjustments> | ((prev: Adjustments) => Adjustments),
      lensIntent?: LensSelectionIntent | null,
      options?: { strictManualIntent?: boolean },
    ) => {
      const strict = options?.strictManualIntent === true;
      if (isProgressiveMutationBlocked(useEditorStore.getState())) return strict ? false : undefined;
      if (strict && !lensIntent) return false;
      let accepted = false;
      setEditor((state) => {
        if (
          strict &&
          (!lensIntent ||
            isProgressiveMutationBlocked(state) ||
            state.selectedImage !== lensIntent.source ||
            state.adjustments !== lensIntent.adjustments ||
            !lensIntent.isCurrent())
        )
          return {};
        const prev = state.adjustments;
        const newAdjustments = typeof value === 'function' ? value(prev) : { ...prev, ...value };
        // Editor/canvas and panels share this timer. A delayed edit belongs to the
        // captured source, even if its originating component has since unmounted.
        debouncedSetHistory(newAdjustments, strict ? lensIntent!.source : (state.selectedImage ?? undefined));
        accepted = true;
        return {
          adjustments: newAdjustments,
          ...(state.showOriginal ? { showOriginal: false, previewOverride: null } : {}),
        };
      });
      return strict ? accepted : undefined;
    },
    [setEditor],
  );

  const handleRotate = useCallback(
    (degrees: number) => {
      const { selectedImage, adjustments } = useEditorStore.getState();
      const increment = degrees > 0 ? 1 : 3;
      const newAspectRatio =
        adjustments.aspectRatio && adjustments.aspectRatio !== 0 ? 1 / adjustments.aspectRatio : null;
      const newOrientationSteps = ((adjustments.orientationSteps || 0) + increment) % 4;
      const newCrop =
        selectedImage?.width && selectedImage?.height
          ? calculateCenteredCrop(selectedImage.width, selectedImage.height, newOrientationSteps, newAspectRatio)
          : null;

      setAdjustments((prev) => ({
        ...prev,
        aspectRatio: newAspectRatio,
        orientationSteps: newOrientationSteps,
        rotation: 0,
        crop: newCrop,
      }));
    },
    [setAdjustments],
  );

  const handleAutoAdjustments = useCallback(async () => {
    const state = useEditorStore.getState();
    const selectedImage = state.selectedImage;
    if (
      !selectedImage?.isReady ||
      isProgressiveMutationBlocked(state) ||
      (selectedImage.openTicket && !currentAccurateOpenKey(state))
    )
      return;
    const path = selectedImage.path;
    const openKey = currentAccurateOpenKey(state);
    try {
      const autoAdjustments: Adjustments = await invoke(Invokes.CalculateAutoAdjustments, {
        jsAdjustments: useEditorStore.getState().adjustments,
        ...(openKey ? { expectedOpenKey: openKey } : {}),
      });
      const current = useEditorStore.getState();
      if (
        current.pendingOpenIntent ||
        current.selectedImage?.path !== path ||
        (openKey ? !isCurrentOpenKey(openKey, current, path, true) : Boolean(current.selectedImage?.openTicket))
      )
        return;
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        ...autoAdjustments,
        sectionVisibility: { ...prev.sectionVisibility, ...autoAdjustments.sectionVisibility },
      }));
    } catch (err) {
      toast.error(`Failed to apply auto adjustments: ${err}`);
    }
  }, [setAdjustments]);

  const toggleShowOriginal = useCallback(() => {
    if (isProgressiveMutationBlocked(useEditorStore.getState())) return;
    setEditor((state) => {
      const isShowing = !state.showOriginal;

      if (isShowing) {
        const override = { ...INITIAL_ADJUSTMENTS };
        const preservedKeys: Array<keyof Adjustments> = [
          'baselineExposure',
          'crop',
          'rotation',
          'flipHorizontal',
          'flipVertical',
          'orientationSteps',
          'aspectRatio',
          'transformDistortion',
          'transformVertical',
          'transformHorizontal',
          'transformRotate',
          'transformAspect',
          'transformScale',
          'transformXOffset',
          'transformYOffset',
          'lensDistortionAmount',
          'lensVignetteAmount',
          'lensTcaAmount',
          'lensDistortionParams',
          'lensMaker',
          'lensModel',
          'lensDistortionEnabled',
          'lensTcaEnabled',
          'lensVignetteEnabled',
        ];

        preservedKeys.forEach((key) => {
          (override as any)[key] = state.adjustments[key];
        });

        return { showOriginal: true, previewOverride: override };
      } else {
        return { showOriginal: false, previewOverride: null };
      }
    });
  }, [setEditor]);

  const handleLutSelect = useCallback(
    async (path: string, isBuiltIn: boolean = false) => {
      const state = useEditorStore.getState();
      if (isProgressiveMutationBlocked(state)) return;
      const selectedPath = state.selectedImage?.path;
      if (!selectedPath) return;
      const openKey = currentAccurateOpenKey(state);
      if (state.selectedImage?.openTicket && !openKey) return;
      const isAndroid = useSettingsStore.getState().osPlatform === 'android';
      const ownsSource = () => {
        const current = useEditorStore.getState();
        return Boolean(
          !current.pendingOpenIntent &&
          current.selectedImage?.path === selectedPath &&
          (openKey ? isCurrentOpenKey(openKey, current, selectedPath, true) : !current.selectedImage?.openTicket),
        );
      };
      try {
        const result: { size: number } = await invoke('load_and_parse_lut', { path });
        if (!ownsSource()) return;
        const name =
          isAndroid && path.startsWith('content://')
            ? await invoke<string>('resolve_android_content_uri_name', { uriStr: path })
            : path.split(/[\\/]/).pop() || 'LUT';
        if (!ownsSource()) return;
        setAdjustments((prev: Adjustments) => ({
          ...prev,
          lutPath: path,
          lutName: name,
          lutSize: result.size,
          lutIntensity: 100,
          lutIsSceneReferred: isBuiltIn,
          sectionVisibility: {
            ...(prev.sectionVisibility || INITIAL_ADJUSTMENTS.sectionVisibility),
            effects: true,
          },
        }));
      } catch (err) {
        toast.error(`Failed to load LUT: ${err}`);
      }
    },
    [setAdjustments],
  );

  const setLutPreviewOverride = useCallback(
    (path: string | null, isBuiltIn: boolean = false) => {
      setEditor((state) => {
        if (!path) return { previewOverride: null };
        const name = path.split(/[\\/]/).pop() || 'LUT';
        return {
          previewOverride: {
            ...state.adjustments,
            lutPath: path,
            lutName: name,
            lutIntensity: state.adjustments.lutIntensity,
            lutIsSceneReferred: isBuiltIn,
          },
        };
      });
    },
    [setEditor],
  );

  const handleResetAdjustments = useCallback(
    (paths?: string[]) => {
      const { multiSelectedPaths, libraryActivePath, setLibrary } = useLibraryStore.getState();
      const { selectedImage, resetHistory } = useEditorStore.getState();
      const selectedState = useEditorStore.getState();
      const selectedOpenKey = currentAccurateOpenKey(selectedState);
      const pathsToReset = paths || multiSelectedPaths;
      const safePathsToReset = filterProgressiveMutationTargets(pathsToReset, selectedState);
      if (safePathsToReset.length === 0) return;

      safePathsToReset.forEach((p) => globalImageCache.delete(p));
      if (selectedImage && safePathsToReset.includes(selectedImage.path)) debouncedSetHistory.cancel();

      invoke(Invokes.ResetAdjustmentsForPaths, { paths: safePathsToReset })
        .then(() => {
          const current = useEditorStore.getState();
          const selectedStillCurrent = Boolean(
            selectedImage &&
            current.selectedImage?.path === selectedImage.path &&
            !current.pendingOpenIntent &&
            (selectedOpenKey
              ? isCurrentOpenKey(selectedOpenKey, current, selectedImage.path, true)
              : !current.selectedImage?.openTicket && current.selectedImage?.isReady),
          );
          if (libraryActivePath && safePathsToReset.includes(libraryActivePath))
            setLibrary({ libraryActiveAdjustments: { ...INITIAL_ADJUSTMENTS } });
          if (selectedImage && selectedStillCurrent && safePathsToReset.includes(selectedImage.path)) {
            const aspect =
              selectedImage.width && selectedImage.height ? selectedImage.width / selectedImage.height : null;
            const resetData = { ...INITIAL_ADJUSTMENTS, aspectRatio: aspect, aiPatches: [] };
            resetHistory(resetData);
            setEditor({ adjustments: resetData });
          }
        })
        .catch((err) => toast.error(`Failed to reset adjustments: ${err}`));
    },
    [setEditor],
  );

  const handleAutoLensCorrection = useCallback(
    (paths?: string[]) => {
      const { multiSelectedPaths, libraryActivePath, setLibrary } = useLibraryStore.getState();
      const { selectedImage, resetHistory } = useEditorStore.getState();
      const selectedState = useEditorStore.getState();
      const selectedOpenKey = currentAccurateOpenKey(selectedState);

      const pathsToUpdate =
        paths && paths.length > 0
          ? paths
          : multiSelectedPaths.length > 0
            ? multiSelectedPaths
            : selectedImage
              ? [selectedImage.path]
              : [];

      if (pathsToUpdate.length === 0) return;

      const safePathsToUpdate = filterProgressiveMutationTargets(pathsToUpdate, selectedState);
      if (safePathsToUpdate.length === 0) return;

      safePathsToUpdate.forEach((p) => globalImageCache.delete(p));

      invoke('apply_auto_lens_correction_to_paths', { paths: safePathsToUpdate })
        .then(async () => {
          const current = useEditorStore.getState();
          const selectedStillCurrent = Boolean(
            selectedImage &&
            current.selectedImage?.path === selectedImage.path &&
            !current.pendingOpenIntent &&
            (selectedOpenKey
              ? isCurrentOpenKey(selectedOpenKey, current, selectedImage.path, true)
              : !current.selectedImage?.openTicket && current.selectedImage?.isReady),
          );
          if (selectedImage && selectedStillCurrent && safePathsToUpdate.includes(selectedImage.path)) {
            const meta: any = await invoke(Invokes.LoadMetadata, { path: selectedImage.path });
            const afterMetadata = useEditorStore.getState();
            const stillSelected =
              selectedStillCurrent &&
              afterMetadata.selectedImage?.path === selectedImage.path &&
              !afterMetadata.pendingOpenIntent &&
              (selectedOpenKey
                ? isCurrentOpenKey(selectedOpenKey, afterMetadata, selectedImage.path, true)
                : !afterMetadata.selectedImage?.openTicket && afterMetadata.selectedImage?.isReady);
            if (stillSelected && meta.adjustments && !meta.adjustments.is_null) {
              const normalized = normalizeLoadedAdjustments(meta.adjustments);
              setEditor({ adjustments: normalized });
              resetHistory(normalized);
            }
          }
          if (libraryActivePath && safePathsToUpdate.includes(libraryActivePath)) {
            const meta: any = await invoke(Invokes.LoadMetadata, { path: libraryActivePath });
            if (meta.adjustments && !meta.adjustments.is_null) {
              setLibrary({ libraryActiveAdjustments: normalizeLoadedAdjustments(meta.adjustments) });
            }
          }
        })
        .catch((err) => toast.error(`Failed to apply auto lens correction: ${err}`));
    },
    [setEditor],
  );

  const handleCopyAdjustments = useCallback(async (pathOrEvent?: string | any) => {
    const pathOverride = typeof pathOrEvent === 'string' ? pathOrEvent : undefined;
    const { selectedImage, adjustments } = useEditorStore.getState();
    const { libraryActivePath, multiSelectedPaths } = useLibraryStore.getState();
    let sourceAdjustments: any = null;

    const pathToCopyFrom =
      pathOverride || (selectedImage ? selectedImage.path : libraryActivePath || multiSelectedPaths[0]);

    const sourceState = useEditorStore.getState();
    if (
      sourceState.pendingOpenIntent ||
      (selectedImage?.openTicket && !currentAccurateOpenKey(sourceState) && pathToCopyFrom === selectedImage.path)
    )
      return;

    if (selectedImage && pathToCopyFrom === selectedImage.path) {
      sourceAdjustments = adjustments;
    } else if (pathToCopyFrom) {
      try {
        const meta: any = await invoke(Invokes.LoadMetadata, { path: pathToCopyFrom });
        if (meta?.adjustments && !meta.adjustments.is_null) {
          sourceAdjustments = normalizeLoadedAdjustments(meta.adjustments);
        } else {
          sourceAdjustments = INITIAL_ADJUSTMENTS;
        }
      } catch (err) {
        toast.error(`Failed to load metadata for copying: ${err}`);
        return;
      }
    }

    if (!sourceAdjustments) return;

    const adjustmentsToCopy: any = {};

    for (const key of COPYABLE_ADJUSTMENT_KEYS) {
      if (Object.prototype.hasOwnProperty.call(sourceAdjustments, key)) {
        adjustmentsToCopy[key] = structuredClone(sourceAdjustments[key]);
      }
    }
    useEditorStore.getState().setEditor({ copiedAdjustments: adjustmentsToCopy });
    useProcessStore.getState().setProcess({ isCopied: true });
  }, []);

  const handlePasteAdjustments = useCallback(
    (paths?: string[]) => {
      const { copiedAdjustments, selectedImage, adjustments } = useEditorStore.getState();
      const selectedState = useEditorStore.getState();
      const selectedOpenKey = currentAccurateOpenKey(selectedState);
      const { multiSelectedPaths } = useLibraryStore.getState();
      const { appSettings } = useSettingsStore.getState();
      const { setProcess } = useProcessStore.getState();

      if (!copiedAdjustments || !appSettings) return;

      const { mode, includedAdjustments } = appSettings.copyPasteSettings;
      const adjustmentsToApply: Partial<Adjustments> = {};

      for (const key of includedAdjustments) {
        if (Object.prototype.hasOwnProperty.call(copiedAdjustments, key)) {
          const value = copiedAdjustments[key as keyof Adjustments];
          if (mode === PasteMode.Merge) {
            const defaultValue = INITIAL_ADJUSTMENTS[key as keyof Adjustments];
            if (JSON.stringify(value) !== JSON.stringify(defaultValue))
              adjustmentsToApply[key as keyof Adjustments] = value;
          } else {
            adjustmentsToApply[key as keyof Adjustments] = value;
          }
        }
      }

      if (includedAdjustments.includes(LensAdjustment.LensMaker)) {
        if (!adjustmentsToApply.lensMaker) {
          adjustmentsToApply.lensDistortionParams = null;
        }
      }

      if (Object.keys(adjustmentsToApply).length === 0) {
        setProcess({ isPasted: true });
        return;
      }

      const pathsToUpdate =
        paths || (multiSelectedPaths.length > 0 ? multiSelectedPaths : selectedImage ? [selectedImage.path] : []);
      const safePathsToUpdate = filterProgressiveMutationTargets(pathsToUpdate, selectedState);
      if (safePathsToUpdate.length === 0) return;

      safePathsToUpdate.forEach((p) => globalImageCache.delete(p));

      if (selectedImage && safePathsToUpdate.includes(selectedImage.path)) {
        setAdjustments({ ...adjustments, ...adjustmentsToApply });
      }

      invoke(Invokes.ApplyAdjustmentsToPaths, { paths: safePathsToUpdate, adjustments: adjustmentsToApply })
        .then(() => {
          const current = useEditorStore.getState();
          const selectedStillCurrent = Boolean(
            selectedImage &&
            current.selectedImage?.path === selectedImage.path &&
            !current.pendingOpenIntent &&
            (selectedOpenKey
              ? isCurrentOpenKey(selectedOpenKey, current, selectedImage.path, true)
              : !current.selectedImage?.openTicket && current.selectedImage?.isReady),
          );
          if (selectedImage && selectedStillCurrent && safePathsToUpdate.includes(selectedImage.path)) {
            invoke('load_metadata', { path: selectedImage.path }).then((meta: any) => {
              const afterMetadata = useEditorStore.getState();
              const stillSelected =
                afterMetadata.selectedImage?.path === selectedImage.path &&
                !afterMetadata.pendingOpenIntent &&
                (selectedOpenKey
                  ? isCurrentOpenKey(selectedOpenKey, afterMetadata, selectedImage.path, true)
                  : !afterMetadata.selectedImage?.openTicket && afterMetadata.selectedImage?.isReady);
              if (stillSelected && meta.adjustments) {
                setAdjustments((prev: any) => ({
                  ...prev,
                  lensMaker: meta.adjustments.lensMaker,
                  lensModel: meta.adjustments.lensModel,
                  lensDistortionParams: meta.adjustments.lensDistortionParams,
                }));
              }
            });
          }
        })
        .catch((err) => toast.error(`Failed to paste adjustments: ${err}`));

      setProcess({ isPasted: true });
    },
    [setAdjustments],
  );

  const handleZoomChange = useCallback((zoomValue: number, fitToWindow: boolean = false) => {
    if (isProgressiveMutationBlocked(useEditorStore.getState())) return;
    const { originalSize, baseRenderSize, adjustments } = useEditorStore.getState();
    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1;
    let targetZoomPercent: number;

    const orientationSteps = adjustments.orientationSteps || 0;
    const isSwapped = orientationSteps === 1 || orientationSteps === 3;
    const effectiveOriginalWidth = isSwapped ? originalSize.height : originalSize.width;
    const effectiveOriginalHeight = isSwapped ? originalSize.width : originalSize.height;

    if (fitToWindow) {
      if (
        effectiveOriginalWidth > 0 &&
        effectiveOriginalHeight > 0 &&
        baseRenderSize.width > 0 &&
        baseRenderSize.height > 0
      ) {
        const originalAspect = effectiveOriginalWidth / effectiveOriginalHeight;
        const baseAspect = baseRenderSize.width / baseRenderSize.height;
        targetZoomPercent =
          originalAspect > baseAspect
            ? baseRenderSize.width / effectiveOriginalWidth
            : baseRenderSize.height / effectiveOriginalHeight;
      } else {
        targetZoomPercent = 1.0;
      }
    } else {
      targetZoomPercent = zoomValue / dpr;
    }

    targetZoomPercent = Math.max(0.1 / dpr, Math.min(2.0, targetZoomPercent));

    let transformZoom = 1.0;
    if (
      effectiveOriginalWidth > 0 &&
      effectiveOriginalHeight > 0 &&
      baseRenderSize.width > 0 &&
      baseRenderSize.height > 0
    ) {
      const originalAspect = effectiveOriginalWidth / effectiveOriginalHeight;
      const baseAspect = baseRenderSize.width / baseRenderSize.height;
      if (originalAspect > baseAspect) {
        transformZoom = (targetZoomPercent * effectiveOriginalWidth) / baseRenderSize.width;
      } else {
        transformZoom = (targetZoomPercent * effectiveOriginalHeight) / baseRenderSize.height;
      }
    }
    useEditorStore.getState().setEditor({ zoom: transformZoom });
  }, []);

  return {
    setAdjustments,
    handleRotate,
    handleAutoAdjustments,
    handleLutSelect,
    setLutPreviewOverride,
    handleResetAdjustments,
    handleAutoLensCorrection,
    handleCopyAdjustments,
    handlePasteAdjustments,
    handleZoomChange,
    toggleShowOriginal,
  };
}

/** Shared with manual straighten: preserve its rotation/crop arithmetic exactly. */
export function straightenedAdjustments(
  prev: Adjustments,
  selectedImage: { width: number; height: number } | null,
  angleCorrection: number,
): Adjustments {
  const newRotation = (prev.rotation || 0) + angleCorrection;
  const newCrop =
    selectedImage?.width && selectedImage?.height
      ? calculateAutoCropForRotation(
          selectedImage.width,
          selectedImage.height,
          prev.orientationSteps || 0,
          prev.aspectRatio,
          newRotation,
          prev.crop,
          angleCorrection,
        )
      : prev.crop;
  return { ...prev, rotation: newRotation, crop: newCrop };
}

export function autoStraightenUnavailable(
  state = useEditorStore.getState(),
  settings = useSettingsStore.getState().appSettings,
): string | null {
  const proof = state.finalPreviewProof;
  if (useUIStore.getState().activeView !== 'editor') return 'Open the photo in the editor first.';
  if (!state.selectedImage?.isReady || isProgressiveMutationBlocked(state) || state.pendingOpenIntent)
    return 'Wait for the current photo to finish loading.';
  if (
    state.isSliderDragging ||
    state.isRotationActive ||
    state.isStraightenActive ||
    state.isGuidedPerspectiveActive ||
    state.liveRotation != null ||
    state.showOriginal ||
    state.previewOverride
  )
    return 'Finish the current adjustment first.';
  const a = state.adjustments;
  const perspectiveKeys = [
    'transformVertical',
    'transformHorizontal',
    'transformRotate',
    'transformAspect',
    'transformXOffset',
    'transformYOffset',
  ] as const;
  if (
    perspectiveKeys.some((key) => (a[key] ?? 0) !== 0) ||
    (a.transformScale ?? 100) !== 100 ||
    a.guidedPerspective?.enabled
  )
    return 'Auto straighten is unavailable with perspective transforms.';
  if (!Number.isFinite(a.rotation ?? 0) || Math.abs(a.rotation ?? 0) > 45)
    return 'Keep fine rotation within 45 degrees.';
  if (
    !proof ||
    !proof.url.startsWith('blob:') ||
    state.finalPreviewUrl !== proof.url ||
    state.selectedImage !== proof.source ||
    state.adjustments !== proof.adjustments ||
    settings !== proof.settings ||
    !Number.isSafeInteger(proof.jobId) ||
    typeof proof.isCurrent !== 'function' ||
    !proof.isCurrent()
  )
    return 'Wait for a complete current preview. Native-display frames are not supported.';
  if (
    proof.openKey
      ? !isCurrentOpenKey(proof.openKey, state, proof.source.path, true)
      : Boolean(state.selectedImage.openTicket || state.selectedImage.openKey)
  )
    return 'Wait for a complete current preview.';
  return null;
}

/** One caller-owned operation; cancellation discards the result and does not queue another. */
export function useAutoStraighten() {
  const state = useEditorStore();
  const settings = useSettingsStore((s) => s.appSettings);
  const { setAdjustments } = useEditorActions();
  const busy = state.autoStraightenBusy;
  const [feedback, setFeedback] = useState('');
  const inFlight = useRef(false);
  const mounted = useRef(true);
  const decodingImage = useRef<HTMLImageElement | null>(null);
  const generation = useRef(0);
  const activeProof = useRef<FinalPreviewProof | null>(null);
  const cancel = useCallback(() => {
    generation.current += 1;
    if (decodingImage.current) {
      decodingImage.current.src = '';
      decodingImage.current = null;
    }
    if (mounted.current) setFeedback('Auto straighten cancelled.');
  }, []);
  useEffect(() => {
    mounted.current = true;
    const changed = () => {
      const proof = activeProof.current;
      if (proof && (useEditorStore.getState().finalPreviewProof !== proof || autoStraightenUnavailable())) {
        generation.current += 1;
        activeProof.current = null;
        if (decodingImage.current) {
          decodingImage.current.src = '';
          decodingImage.current = null;
        }
      }
    };
    const offEditor = useEditorStore.subscribe(changed);
    const offSettings = useSettingsStore.subscribe(changed);
    const offUi = useUIStore.subscribe(changed);
    return () => {
      mounted.current = false;
      generation.current += 1;
      if (decodingImage.current) {
        decodingImage.current.src = '';
        decodingImage.current = null;
      }
      activeProof.current = null;
      offEditor();
      offSettings();
      offUi();
    };
  }, []);
  const run = useCallback(async () => {
    if (inFlight.current || useEditorStore.getState().autoStraightenBusy) return;
    const reason = autoStraightenUnavailable();
    if (reason) {
      setFeedback(reason);
      return;
    }
    const proof = useEditorStore.getState().finalPreviewProof!;
    const token = ++generation.current;
    const current = () =>
      mounted.current &&
      token === generation.current &&
      useEditorStore.getState().finalPreviewProof === proof &&
      !autoStraightenUnavailable();
    inFlight.current = true;
    activeProof.current = proof;
    useEditorStore.getState().setEditor({ autoStraightenBusy: true });
    setFeedback('');
    let image: HTMLImageElement | null = null;
    let canvas: HTMLCanvasElement | null = null;
    try {
      if (!current()) return;
      image = new Image();
      image.src = proof.url;
      decodingImage.current = image;
      await image.decode();
      if (decodingImage.current === image) decodingImage.current = null;
      if (!current()) return;
      const w = image.naturalWidth,
        h = image.naturalHeight;
      if (!Number.isSafeInteger(w) || !Number.isSafeInteger(h) || w <= 0 || h <= 0)
        throw new Error('Invalid preview dimensions.');
      const scale = Math.min(1, 512 / Math.max(w, h));
      const width = Math.max(1, Math.round(w * scale)),
        height = Math.max(1, Math.round(h * scale));
      canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      if (!context) throw new Error('Preview analysis is unavailable.');
      context.drawImage(image, 0, 0, width, height);
      const rgba = context.getImageData(0, 0, width, height).data;
      if (rgba.length !== width * height * 4 || rgba.length > 1024 * 1024)
        throw new Error('Invalid bounded preview data.');
      if (!current()) return;
      image.src = '';
      image = null;
      canvas.width = 0;
      canvas.height = 0;
      canvas = null;
      const result = await invoke<{ correctionDegrees: number; confidence: number } | null>(
        Invokes.CalculateAutoStraighten,
        {
          path: proof.source.path,
          width,
          height,
          rgba: Array.from(rgba),
          ...(proof.openKey ? { expectedOpenKey: proof.openKey } : {}),
        },
      );
      if (!current()) return;
      if (result === null) {
        setFeedback('No clear level lines found. No changes made.');
        return;
      }
      if (
        !Number.isFinite(result.correctionDegrees) ||
        Math.abs(result.correctionDegrees) > 10 ||
        !Number.isFinite(result.confidence) ||
        result.confidence < 0.85 ||
        result.confidence > 1
      )
        throw new Error('Invalid auto straighten estimate.');
      const rotation = (proof.adjustments.rotation || 0) + result.correctionDegrees;
      if (!Number.isFinite(rotation) || Math.abs(rotation) > 45) {
        setFeedback('Correction would exceed 45 degrees. No changes made.');
        return;
      }
      if (result.correctionDegrees === 0) {
        setFeedback('Already level. No changes made.');
        return;
      }
      debouncedSetHistory.flush();
      if (!current()) return;
      activeProof.current = null;
      const accepted = setAdjustments(
        (prev) => straightenedAdjustments(prev, proof.source, result.correctionDegrees),
        { path: proof.source.path, source: proof.source, adjustments: proof.adjustments, isCurrent: current },
        { strictManualIntent: true },
      );
      if (accepted) {
        debouncedSetHistory.flush();
        if (mounted.current) setFeedback('Straightened.');
      }
    } catch (error) {
      if (current()) setFeedback(`Auto straighten failed: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      if (decodingImage.current === image) decodingImage.current = null;
      if (image) image.src = '';
      if (canvas) {
        canvas.width = 0;
        canvas.height = 0;
      }
      activeProof.current = null;
      inFlight.current = false;
      useEditorStore.getState().setEditor({ autoStraightenBusy: false });
    }
  }, [setAdjustments]);
  return {
    run,
    cancel,
    busy,
    canCancel: inFlight.current,
    feedback,
    unavailable: autoStraightenUnavailable(state, settings),
  };
}
