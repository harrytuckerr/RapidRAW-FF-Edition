import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Status } from '../components/ui/ExportImportProperties';
import {
  IMPORT_EVENT_NAMES,
  IMPORT_UNKNOWN_RESULT_MESSAGE,
  ImportEffect,
  beginImportListenerGeneration,
  consumeImportEvent,
  endImportListenerGeneration,
  markImportListenersReady,
} from '../utils/imageImport';
import { useProcessStore } from '../store/useProcessStore';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { currentAccurateOpenKey, parseOpenKey, sameOpenKey, sourceRenderPolicy } from '../utils/progressiveOpen';

interface TauriListenerProps {
  refreshAllFolderTrees: () => void;
  handleSelectSubfolder: (path: string, isNewRoot?: boolean, preloadedImages?: any[], expandParents?: boolean) => void;
  refreshImageList: () => void;
  markGenerated: (path: string) => void;
}

export function useTauriListeners({
  refreshAllFolderTrees,
  handleSelectSubfolder,
  refreshImageList,
  markGenerated,
}: TauriListenerProps) {
  const refs = useRef({ refreshAllFolderTrees, handleSelectSubfolder, refreshImageList, markGenerated });

  useEffect(() => {
    refs.current = { refreshAllFolderTrees, handleSelectSubfolder, refreshImageList, markGenerated };
  });

  const thumbnailBuffer = useRef<Record<string, string>>({});
  const mediumThumbnailBuffer = useRef<Record<string, string>>({});
  const ratingBuffer = useRef<Record<string, number>>({});
  const editStatusBuffer = useRef<Record<string, boolean>>({});
  const flushHandle = useRef<number | null>(null);

  useEffect(() => {
    let isEffectActive = true;
    const denoiseJobs = new Map<string, bigint>();

    const flushThumbnailBatch = () => {
      flushHandle.current = null;
      if (!isEffectActive) return;

      const pendingThumbs = thumbnailBuffer.current;
      const pendingMediumThumbs = mediumThumbnailBuffer.current;
      const pendingRatings = ratingBuffer.current;
      const pendingEdits = editStatusBuffer.current;

      thumbnailBuffer.current = {};
      mediumThumbnailBuffer.current = {};
      ratingBuffer.current = {};
      editStatusBuffer.current = {};

      if (Object.keys(pendingThumbs).length > 0) {
        useProcessStore.getState().setProcess((state) => ({
          thumbnails: { ...state.thumbnails, ...pendingThumbs },
          mediumThumbnails: { ...state.mediumThumbnails, ...pendingMediumThumbs },
        }));
      }

      if (Object.keys(pendingRatings).length > 0 || Object.keys(pendingEdits).length > 0) {
        useLibraryStore.getState().setLibrary((state) => ({
          imageRatings: { ...state.imageRatings, ...pendingRatings },
          imageList:
            Object.keys(pendingEdits).length > 0
              ? state.imageList.map((img) =>
                  pendingEdits[img.path] !== undefined ? { ...img, is_edited: pendingEdits[img.path] } : img,
                )
              : state.imageList,
        }));
      }
    };

    const scheduleFlush = () => {
      if (flushHandle.current !== null) return;
      flushHandle.current = requestAnimationFrame(flushThumbnailBatch);
    };

    const listeners = [
      listen('preview-update-uncropped', (event: any) => {
        if (!isEffectActive) return;
        const state = useEditorStore.getState();
        if (
          state.selectedImage?.openTicket ||
          state.progressiveOpen?.phase === 'Pending' ||
          state.progressiveOpen?.phase === 'Refining'
        )
          return;
        if (sourceRenderPolicy(state) !== 'legacy-native') return;
        if (state.selectedImage?.isReady) state.setEditor({ uncroppedAdjustedPreviewUrl: event.payload });
      }),
      listen('analytics-update', (event: any) => {
        if (!isEffectActive) return;
        const state = useEditorStore.getState();
        const payload = event.payload;
        if (!payload || payload.path !== state.selectedImage?.path) return;
        if (payload.openKey != null) {
          if (sourceRenderPolicy(state) !== 'encoded') return;
          const currentKey = currentAccurateOpenKey(state);
          const eventKey = parseOpenKey(payload.openKey);
          if (!currentKey || !eventKey || !sameOpenKey(currentKey, eventKey)) return;
        } else if (sourceRenderPolicy(state) !== 'legacy-native') {
          return;
        }
        const update: { histogram?: any; waveform?: any } = {};
        if (payload.histogram != null) update.histogram = payload.histogram;
        if (payload.waveform != null) update.waveform = payload.waveform;
        state.setEditor(update);
      }),
      listen('open-with-file', (event: any) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ initialFileToOpen: event.payload as string });
      }),
      listen('external-edit-session', (event: any) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ externalEditSession: event.payload });
      }),
      listen('thumbnail-progress', (event: any) => {
        if (isEffectActive)
          useProcessStore
            .getState()
            .setProcess({ thumbnailProgress: { current: event.payload.current, total: event.payload.total } });
      }),
      listen('thumbnail-generation-complete', () => {
        if (isEffectActive) useProcessStore.getState().setProcess({ thumbnailProgress: { current: 0, total: 0 } });
      }),
      listen('thumbnail-generated', (event: any) => {
        if (!isEffectActive) return;
        const { path, thumbnailPath, previewPath, rating, is_edited, data } = event.payload;

        if (thumbnailPath && previewPath) {
          thumbnailBuffer.current[path] = convertFileSrc(thumbnailPath.replace(/\\/g, '/'));
          mediumThumbnailBuffer.current[path] = convertFileSrc(previewPath.replace(/\\/g, '/'));
          refs.current.markGenerated(path);
        } else if (data) {
          thumbnailBuffer.current[path] = data;
          mediumThumbnailBuffer.current[path] = data;
          refs.current.markGenerated(path);
        }
        if (rating !== undefined) {
          ratingBuffer.current[path] = rating;
        }
        if (is_edited !== undefined) {
          editStatusBuffer.current[path] = is_edited;
        }
        if (thumbnailPath || data || rating !== undefined || is_edited !== undefined) {
          scheduleFlush();
        }
      }),
      listen('image-metadata-loaded', (event: any) => {
        if (!isEffectActive) return;
        const { path, rating, flag, is_edited, tags } = event.payload;

        useLibraryStore.getState().setLibrary((state) => ({
          imageRatings: { ...state.imageRatings, [path]: rating },
          imageList: state.imageList.map((img) =>
            img.path === path ? { ...img, flag, is_edited, tags: tags ?? img.tags } : img,
          ),
        }));
      }),
      listen('ai-model-download-start', (event: any) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ aiModelDownloadStatus: event.payload });
      }),
      listen('ai-model-download-finish', () => {
        if (isEffectActive) useProcessStore.getState().setProcess({ aiModelDownloadStatus: null });
      }),
      listen('indexing-started', () => {
        if (isEffectActive)
          useProcessStore.getState().setProcess({ isIndexing: true, indexingProgress: { current: 0, total: 0 } });
      }),
      listen('indexing-progress', (event: any) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ indexingProgress: event.payload });
      }),
      listen('indexing-finished', () => {
        if (isEffectActive) {
          useProcessStore.getState().setProcess({ isIndexing: false, indexingProgress: { current: 0, total: 0 } });
          const currentPath = useLibraryStore.getState().currentFolderPath;
          if (currentPath) {
            refs.current.refreshImageList();
          }
        }
      }),
      listen('batch-export-progress', (event: any) => {
        if (isEffectActive) useProcessStore.getState().setExportState({ progress: event.payload });
      }),
      listen('export-complete', () => {
        if (isEffectActive) useProcessStore.getState().setExportState({ status: Status.Success });
      }),
      listen('export-error', (event: any) => {
        if (isEffectActive)
          useProcessStore.getState().setExportState({
            status: Status.Error,
            errorMessage: typeof event.payload === 'string' ? event.payload : 'Unknown error',
          });
      }),
      listen('export-cancelling', () => {
        if (isEffectActive) useProcessStore.getState().setExportState({ status: Status.Cancelling });
      }),
      listen('export-cancelled', () => {
        if (isEffectActive) useProcessStore.getState().setExportState({ status: Status.Cancelled });
      }),
      listen('denoise-progress', (event: any) => {
        if (isEffectActive)
          useUIStore.getState().setUI((state) => ({
            denoiseModalState: { ...state.denoiseModalState, progressMessage: event.payload as string },
          }));
      }),
      listen('denoise-complete', (event: any) => {
        if (isEffectActive) {
          const payload = event.payload;
          const isObject = typeof payload === 'object' && payload !== null;
          useUIStore.getState().setUI((state) => ({
            denoiseModalState: {
              ...state.denoiseModalState,
              isProcessing: false,
              previewBase64: isObject ? payload.denoised : payload,
              originalBase64: isObject ? payload.original : null,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('denoise-error', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            denoiseModalState: {
              ...state.denoiseModalState,
              isProcessing: false,
              error: String(event.payload),
              progressMessage: null,
            },
          }));
        }
      }),
      listen<{
        path: string;
        openKey: unknown;
        job: string;
        baseKey: string;
        profile: unknown;
        progress: number;
        error?: string;
      }>('ai-denoise-progress', (event) => {
        if (!isEffectActive) return;
        const payload = event.payload;
        const state = useEditorStore.getState();
        if (
          !payload ||
          payload.path !== state.selectedImage?.path ||
          typeof payload.job !== 'string' ||
          !/^(0|[1-9][0-9]{0,19})$/.test(payload.job) ||
          typeof payload.baseKey !== 'string' ||
          !/^[0-9a-f]{64}$/.test(payload.baseKey) ||
          !Number.isFinite(payload.progress) ||
          payload.progress < 0 ||
          payload.progress > 1
        )
          return;
        const key = parseOpenKey(payload.openKey);
        if (!key || !sameOpenKey(key, currentAccurateOpenKey(state))) return;
        const job = BigInt(payload.job);
        if (job > 18446744073709551615n || job < (denoiseJobs.get(payload.path) ?? 0n)) return;
        const selected = state.adjustments.profile;
        const profile = payload.profile as typeof selected;
        if (
          JSON.stringify(profile?.base ?? null) !== JSON.stringify(selected?.base ?? null) ||
          JSON.stringify(profile?.look ?? null) !== JSON.stringify(selected?.look ?? null) ||
          JSON.stringify(profile?.amount ?? null) !== JSON.stringify(selected?.amount ?? null)
        )
          return;
        if (!state.adjustments.aiDenoiseEnabled || state.adjustments.aiDenoiseAmount <= 0) return;
        denoiseJobs.set(payload.path, job);
        // The native preview invoke awaits the matching result and owns errors.
        // Progress never commits pixels, toggles an edit, or queues a second render.
        state.setEditor((current) => {
          if (
            current.selectedImage !== state.selectedImage ||
            !sameOpenKey(currentAccurateOpenKey(current), key) ||
            current.adjustments !== state.adjustments
          )
            return {};
          const aiDenoiseProgress = { ...current.aiDenoiseProgress };
          if (payload.progress === 1 || typeof payload.error === 'string') delete aiDenoiseProgress[payload.path];
          else aiDenoiseProgress[payload.path] = payload.progress;
          return { aiDenoiseProgress };
        });
      }),
      listen('wgpu-frame-ready', (event: any) => {
        if (!isEffectActive) return;
        const state = useEditorStore.getState();
        const payload = event.payload;
        if (!payload || payload.path !== state.selectedImage?.path || payload.openKey != null) return;
        if (sourceRenderPolicy(state) !== 'legacy-native') return;
        state.setEditor({ hasRenderedFirstFrame: true });
      }),
      listen('panorama-progress', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => {
            if (state.panoramaModalState.finalImageBase64 || state.panoramaModalState.error) return state;
            return { panoramaModalState: { ...state.panoramaModalState, progressMessage: event.payload } };
          });
        }
      }),
      listen('panorama-complete', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            panoramaModalState: {
              ...state.panoramaModalState,
              error: null,
              finalImageBase64: event.payload.base64,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('panorama-error', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            panoramaModalState: {
              ...state.panoramaModalState,
              error: String(event.payload),
              finalImageBase64: null,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('hdr-progress', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            hdrModalState: {
              ...state.hdrModalState,
              error: null,
              finalImageBase64: null,
              isOpen: true,
              progressMessage: event.payload,
            },
          }));
        }
      }),
      listen('hdr-complete', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            hdrModalState: {
              ...state.hdrModalState,
              error: null,
              finalImageBase64: event.payload.base64,
              isProcessing: false,
              progressMessage: 'Hdr Ready',
            },
          }));
        }
      }),
      listen('hdr-error', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            hdrModalState: {
              ...state.hdrModalState,
              error: String(event.payload),
              finalImageBase64: null,
              isProcessing: false,
              progressMessage: 'An error occurred.',
            },
          }));
        }
      }),
      listen('focus-stack-progress', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => {
            if (state.focusStackModalState.finalImageBase64 || state.focusStackModalState.error) return state;
            return { focusStackModalState: { ...state.focusStackModalState, progressMessage: event.payload } };
          });
        }
      }),
      listen('focus-stack-complete', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            focusStackModalState: {
              ...state.focusStackModalState,
              error: null,
              finalImageBase64: event.payload.base64,
              depthMapBase64: event.payload.depthMap,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('focus-stack-error', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            focusStackModalState: {
              ...state.focusStackModalState,
              error: String(event.payload),
              finalImageBase64: null,
              depthMapBase64: null,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('culling-start', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            cullingModalState: {
              ...state.cullingModalState,
              isOpen: true,
              progress: { current: 0, total: event.payload, stage: 'Initializing...' },
              suggestions: null,
              error: null,
            },
          }));
        }
      }),
      listen('culling-progress', (event: any) => {
        if (isEffectActive) {
          useUIStore
            .getState()
            .setUI((state) => ({ cullingModalState: { ...state.cullingModalState, progress: event.payload } }));
        }
      }),
      listen('culling-complete', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            cullingModalState: { ...state.cullingModalState, progress: null, suggestions: event.payload },
          }));
        }
      }),
      listen('culling-error', (event: any) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            cullingModalState: { ...state.cullingModalState, progress: null, error: String(event.payload) },
          }));
        }
      }),
    ];

    // Import listeners form their own group: imports are admitted only after all
    // four are registered for this generation (see utils/imageImport).
    const importUnlisteners: Array<() => void> = [];
    let importRegistrationOpen = true;

    const runRefresh = (refresh: () => unknown) => {
      try {
        void Promise.resolve(refresh()).catch((err) => console.error('Folder refresh after import failed:', err));
      } catch (err) {
        console.error('Folder refresh after import failed:', err);
      }
    };

    const refreshAfterImport = () => {
      runRefresh(() => refs.current.refreshAllFolderTrees());
      const currentPath = useLibraryStore.getState().currentFolderPath;
      if (currentPath) {
        runRefresh(() => refs.current.handleSelectSubfolder(currentPath, false));
      }
    };

    const applyImportEffect = (effect: ImportEffect) => {
      const { setImportState } = useProcessStore.getState();
      switch (effect.kind) {
        case 'progress':
          setImportState({
            status: Status.Importing,
            path: effect.path,
            progress: { current: effect.current, total: effect.total },
          });
          break;
        case 'itemError':
          // Item failures are shown but the run stays Importing until its terminal result.
          setImportState({ status: Status.Importing, errorMessage: effect.errorMessage });
          break;
        case 'terminal':
          setImportState({
            status: effect.failed > 0 ? Status.Error : Status.Success,
            errorMessage: effect.errorMessage,
            path: '',
            progress: { current: effect.total, total: effect.total },
          });
          if (effect.failed > 0) toast.error(effect.errorMessage, { toastId: `import-${effect.runId}` });
          refreshAfterImport();
          break;
        case 'unknownResult':
          setImportState({ status: Status.Error, errorMessage: IMPORT_UNKNOWN_RESULT_MESSAGE, path: '' });
          toast.error(IMPORT_UNKNOWN_RESULT_MESSAGE, { toastId: `import-${effect.runId}` });
          if (effect.refresh) refreshAfterImport();
          break;
        default:
          break;
      }
    };

    const { generation: importGeneration, effect: supersededRun } = beginImportListenerGeneration();
    applyImportEffect(supersededRun);

    const disposeImportListener = (unlisten: () => void) => {
      try {
        void Promise.resolve(unlisten()).catch((err) => console.error('Could not dispose import listener:', err));
      } catch (err) {
        console.error('Could not dispose import listener:', err);
      }
    };
    const closeImportRegistration = () => {
      importRegistrationOpen = false;
      importUnlisteners.splice(0).forEach(disposeImportListener);
    };

    const importRegistrations = IMPORT_EVENT_NAMES.map((name) =>
      Promise.resolve()
        .then(() =>
          listen<unknown>(name, (event) => {
            if (isEffectActive && importRegistrationOpen)
              applyImportEffect(consumeImportEvent(importGeneration, name, event?.payload));
          }),
        )
        .then(
          (unlisten) => {
            if (importRegistrationOpen) {
              importUnlisteners.push(unlisten);
            } else {
              disposeImportListener(unlisten);
            }
            return true;
          },
          (err) => {
            console.error(`Failed to register ${name} listener:`, err);
            closeImportRegistration();
            return false;
          },
        ),
    );

    Promise.all(importRegistrations).then((registered) => {
      if (importRegistrationOpen && registered.every(Boolean)) markImportListenersReady(importGeneration);
    });

    return () => {
      isEffectActive = false;
      closeImportRegistration();
      applyImportEffect(endImportListenerGeneration(importGeneration));
      if (flushHandle.current !== null) {
        cancelAnimationFrame(flushHandle.current);
        flushHandle.current = null;
      }
      thumbnailBuffer.current = {};
      ratingBuffer.current = {};
      listeners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, []);
}
