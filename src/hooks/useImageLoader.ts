import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'react-toastify';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { Invokes } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from '../utils/adjustments';
import {
  acquireOpenFlight,
  decodeJpegBase64,
  isCurrentOpenKey,
  parseOpenKey,
  parseSourceRenderMode,
  sameOpenKey,
  serializeOpenAdmission,
  sourceRenderPolicy,
  validatePlaceholderReady,
} from '../utils/progressiveOpen';
import type { OpenKey, PlaceholderReady } from '../utils/progressiveOpen';

const PROFILE_BASE_ID = /^[0-9a-f]{64}$/;

/** Native provenance for an unsaved fresh-RAW camera default: exactly `{ base }`. */
export function decodeInitialProfileDefault(value: unknown): string | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record);
  return keys.length === 1 && typeof record.base === 'string' && PROFILE_BASE_ID.test(record.base) ? record.base : null;
}

/**
 * The accurate open already rendered with this base. Make it this open's untouched
 * baseline (history[0]) before the source becomes ready, so viewing never autosaves
 * and the first real edit persists it normally. Anything stale, edited or already
 * selected is left unchanged.
 */
export function adoptInitialProfileDefault(ticket: string, path: string, openKey: OpenKey, base: string): boolean {
  let adopted = false;
  useEditorStore.setState((state) => {
    const current = state.progressiveOpen;
    const selected = state.selectedImage;
    const baseline = state.history[0];
    if (
      !current ||
      current.ticket !== ticket ||
      current.path !== path ||
      (current.phase !== 'Pending' && current.phase !== 'Refining') ||
      !current.openKey ||
      !sameOpenKey(current.openKey, openKey) ||
      selected?.path !== path ||
      selected.openTicket !== ticket ||
      selected.isReady ||
      !selected.openKey ||
      !sameOpenKey(selected.openKey, openKey) ||
      state.history.length !== 1 ||
      state.historyIndex !== 0 ||
      state.adjustments !== baseline ||
      !baseline?.profile ||
      baseline.profile.base !== null ||
      baseline.profile.look !== null
    )
      return state;
    adopted = true;
    const adjustments = { ...baseline, profile: { ...baseline.profile, base } };
    return { adjustments, history: [adjustments], historyIndex: 0 };
  });
  return adopted;
}

export function useImageLoader(cachedEditStateRef: React.RefObject<any>) {
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const adjustments = useEditorStore((s) => s.adjustments);
  const histogram = useEditorStore((s) => s.histogram);
  const waveform = useEditorStore((s) => s.waveform);
  const finalPreviewUrl = useEditorStore((s) => s.finalPreviewUrl);
  const uncroppedAdjustedPreviewUrl = useEditorStore((s) => s.uncroppedAdjustedPreviewUrl);
  const originalSize = useEditorStore((s) => s.originalSize);
  const previewSize = useEditorStore((s) => s.previewSize);
  const hasRenderedFirstFrame = useEditorStore((s) => s.hasRenderedFirstFrame);
  const renderPolicy = useEditorStore(sourceRenderPolicy);
  const progressiveSaveReady = useEditorStore((s) => s.progressiveOpen?.saveReady ?? false);
  const progressiveTicket = useEditorStore((s) => s.progressiveOpen?.ticket ?? null);
  const pendingIntentTicket = useEditorStore((s) => s.pendingOpenIntent?.ticket ?? null);

  const setEditor = useEditorStore((s) => s.setEditor);
  const setLibrary = useLibraryStore((s) => s.setLibrary);
  const appSettings = useSettingsStore((s) => s.appSettings);

  const isWgpuActive =
    appSettings?.useWgpuRenderer !== false && renderPolicy === 'legacy-native' && hasRenderedFirstFrame;

  useEffect(() => {
    const progressivePhase = useEditorStore.getState().progressiveOpen?.phase;
    if (
      !selectedImage?.path ||
      !selectedImage.openTicket ||
      selectedImage.isReady ||
      !progressiveSaveReady ||
      progressiveTicket !== selectedImage.openTicket ||
      pendingIntentTicket ||
      (progressivePhase !== 'Pending' && progressivePhase !== 'Refining')
    )
      return;
    const { path, openTicket: ticket } = selectedImage;

    const ownsIntent = () => {
      const state = useEditorStore.getState();
      return (
        !state.pendingOpenIntent &&
        state.progressiveOpen?.ticket === ticket &&
        state.progressiveOpen.path === path &&
        (state.progressiveOpen.phase === 'Pending' || state.progressiveOpen.phase === 'Refining') &&
        state.selectedImage?.openTicket === ticket &&
        state.selectedImage.path === path
      );
    };
    const cancelNative = (key: OpenKey) => {
      void serializeOpenAdmission(() => invoke(Invokes.CancelImageOpen, { openKey: key })).catch(() => {});
    };
    const cancelCurrentStoreOwner = (key: OpenKey | null) => {
      const state = useEditorStore.getState();
      const current = state.progressiveOpen;
      if (
        !current ||
        current.ticket !== ticket ||
        current.path !== path ||
        (current.phase !== 'Pending' && current.phase !== 'Refining') ||
        state.selectedImage?.path !== path ||
        state.selectedImage.openTicket !== ticket ||
        (key
          ? !sameOpenKey(current.openKey, key) || !sameOpenKey(state.selectedImage.openKey, key)
          : Boolean(current.openKey || state.selectedImage.openKey))
      )
        return;
      state.cancelProgressiveOpen(ticket);
    };

    const flight = acquireOpenFlight(
      ticket,
      async (control) => {
        let unlisten: UnlistenFn | null = null;
        let queuedEvent: PlaceholderReady | null = null;
        let currentKey: OpenKey | null = null;
        let terminal = false;

        const processPlaceholder = async (placeholder: PlaceholderReady) => {
          const state = useEditorStore.getState();
          if (
            !currentKey ||
            !sameOpenKey(placeholder.openKey, currentKey) ||
            !isCurrentOpenKey(currentKey, state, path, false) ||
            !state.progressiveOpen ||
            !state.progressiveOpen.saveReady ||
            state.progressiveOpen.placeholderUsed
          )
            return;
          if (!state.claimProgressivePlaceholder(ticket, path, currentKey)) return;
          const bytes = decodeJpegBase64(placeholder.dataBase64);
          if (!bytes) return;
          const exactBuffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
          let url: string;
          try {
            url = URL.createObjectURL(new Blob([exactBuffer], { type: 'image/jpeg' }));
          } catch {
            return;
          }
          let image: HTMLImageElement | null = null;
          let disposed = false;
          const dispose = () => {
            if (disposed) return;
            disposed = true;
            control.clearPlaceholderCleanup(dispose);
            try {
              if (image) {
                image.onload = null;
                image.onerror = null;
                image.onabort = null;
                try {
                  image.removeAttribute('src');
                } catch {
                  // A detached decode may already have been released by the browser.
                }
              }
            } catch {
              // URL revocation still runs if a browser image implementation rejects detachment.
            } finally {
              URL.revokeObjectURL(url);
            }
          };
          if (!control.registerPlaceholderCleanup(dispose)) {
            dispose();
            return;
          }
          try {
            image = new Image();
            image.src = url;
            await image.decode();
            if (disposed || control.isCancelled() || control.isTerminal()) {
              dispose();
              return;
            }
            if (image.naturalWidth !== placeholder.width || image.naturalHeight !== placeholder.height) {
              dispose();
              return;
            }
            const current = useEditorStore.getState();
            if (
              !ownsIntent() ||
              control.isCancelled() ||
              !isCurrentOpenKey(currentKey, current, path, false) ||
              !current.setProgressivePlaceholder(ticket, path, currentKey, url)
            ) {
              dispose();
              return;
            }
          } catch {
            dispose();
          }
        };

        const onPlaceholder = (payload: unknown) => {
          const fitLimit = appSettings?.editorPreviewResolution || 1920;
          const placeholder = validatePlaceholderReady(payload, fitLimit);
          if (!placeholder || placeholder.openKey.ticket !== ticket || !ownsIntent()) return;
          const state = useEditorStore.getState();
          if (!state.progressiveOpen || state.progressiveOpen.placeholderUsed) return;
          if (!currentKey) {
            if (!queuedEvent) queuedEvent = placeholder;
            return;
          }
          void processPlaceholder(placeholder);
        };

        try {
          if (control.isCancelled() || !ownsIntent()) return;
          if (!useEditorStore.getState().progressiveOpen?.saveReady) return;

          try {
            unlisten = await listen('xtrans-placeholder-ready', (event) => onPlaceholder(event.payload));
            if (control.isCancelled() || !ownsIntent()) {
              unlisten();
              unlisten = null;
              return;
            }
          } catch (error) {
            console.warn('Progressive placeholder listener unavailable; continuing with accurate load only.', error);
          }

          const admitted = await serializeOpenAdmission(async () => {
            if (control.isCancelled() || !ownsIntent() || !useEditorStore.getState().progressiveOpen?.saveReady)
              return null;
            const result: any = await invoke(Invokes.BeginImageOpen, { ticket, path });
            const key = parseOpenKey(result?.openKey);
            if (!key || key.ticket !== ticket || !Object.prototype.hasOwnProperty.call(result, 'metadata')) {
              throw new Error('The image-open admission returned an invalid OpenKey or metadata snapshot.');
            }
            if (control.isCancelled() || !ownsIntent()) {
              control.setKey(key);
              await invoke(Invokes.CancelImageOpen, { openKey: key }).catch(() => {});
              terminal = true;
              control.markTerminal();
              return null;
            }
            currentKey = key;
            control.setKey(key);
            const metadata = result.metadata;
            const initialAdjustments =
              metadata?.adjustments && !metadata.adjustments.is_null
                ? normalizeLoadedAdjustments(metadata.adjustments)
                : { ...INITIAL_ADJUSTMENTS };
            if (
              !useEditorStore.getState().acceptProgressiveAdmission(ticket, path, key, metadata, initialAdjustments)
            ) {
              await invoke(Invokes.CancelImageOpen, { openKey: key }).catch(() => {});
              terminal = true;
              control.markTerminal();
              return null;
            }
            return { key };
          });

          if (!admitted) {
            if (ownsIntent() && !terminal) useEditorStore.getState().cancelProgressiveOpen(ticket);
            terminal = true;
            control.markTerminal();
            return;
          }
          currentKey = admitted.key;
          if (queuedEvent) {
            const queued = queuedEvent;
            queuedEvent = null;
            void processPlaceholder(queued);
          }

          const loadResult: any = await invoke(Invokes.LoadImage, { path, openKey: currentKey });
          const returnedKey = parseOpenKey(loadResult?.openKey);
          if (!returnedKey || !sameOpenKey(returnedKey, currentKey))
            throw new Error('The accurate image response has a stale or malformed OpenKey.');
          const renderMode =
            loadResult &&
            typeof loadResult === 'object' &&
            Object.prototype.hasOwnProperty.call(loadResult, 'renderMode')
              ? parseSourceRenderMode(loadResult.renderMode)
              : null;
          if (!renderMode) throw new Error('The accurate image response has a missing or invalid renderMode.');
          if (control.isCancelled()) {
            terminal = true;
            control.markTerminal();
            return;
          }
          if (!ownsIntent() || !isCurrentOpenKey(currentKey, useEditorStore.getState(), path, false)) {
            cancelNative(currentKey);
            terminal = true;
            control.markTerminal();
            return;
          }
          if (
            !Number.isInteger(loadResult.width) ||
            !Number.isInteger(loadResult.height) ||
            loadResult.width <= 0 ||
            loadResult.height <= 0
          ) {
            throw new Error('The accurate image response has invalid dimensions.');
          }
          const width = loadResult.width;
          const height = loadResult.height;
          const maxSize = appSettings?.editorPreviewResolution || 0;
          let previewSize = { width: 0, height: 0 };
          if (maxSize > 0) {
            const aspect = width / height;
            previewSize =
              width > height
                ? { width: Math.min(width, maxSize), height: Math.round(Math.min(width, maxSize) / aspect) }
                : { height: Math.min(height, maxSize), width: Math.round(Math.min(height, maxSize) * aspect) };
          }
          const initialDefaultBase = decodeInitialProfileDefault(loadResult.initialProfileDefault);
          if (initialDefaultBase) adoptInitialProfileDefault(ticket, path, currentKey, initialDefaultBase);
          if (
            !useEditorStore
              .getState()
              .completeProgressiveLoad(ticket, path, currentKey, { ...loadResult, renderMode, previewSize })
          ) {
            cancelNative(currentKey);
            terminal = true;
            control.markTerminal();
            return;
          }
          useLibraryStore.getState().setLibrary({ isViewLoading: false });
          terminal = true;
          control.markTerminal();
        } catch (error) {
          const current = useEditorStore.getState();
          const ownsCurrentSource = ownsIntent();
          const ownedKey = currentKey && isCurrentOpenKey(currentKey, current, path, false) ? currentKey : null;
          if (ownsCurrentSource) current.failProgressiveOpen(ticket, path, ownedKey, String(error));
          if (ownedKey) cancelNative(ownedKey);
          if (ownsCurrentSource) {
            console.error('Failed to open image:', error);
            toast.error(`Failed to load image: ${error}`);
            useLibraryStore.getState().setLibrary({ isViewLoading: false });
          }
          terminal = true;
          control.markTerminal();
        } finally {
          if (unlisten) unlisten();
          if (!terminal && ownsIntent()) useEditorStore.getState().cancelProgressiveOpen(ticket);
        }
      },
      cancelNative,
      cancelCurrentStoreOwner,
    );

    void flight.promise.catch((error) => {
      console.error('Progressive image-open task failed:', error);
    });
    return () => flight.release();
  }, [selectedImage?.path, selectedImage?.openTicket, progressiveSaveReady, progressiveTicket, pendingIntentTicket]);

  useEffect(() => {
    if (
      selectedImage?.path &&
      !selectedImage.openTicket &&
      selectedImage.isReady &&
      (finalPreviewUrl || isWgpuActive)
    ) {
      cachedEditStateRef.current = {
        adjustments,
        histogram,
        waveform,
        finalPreviewUrl,
        uncroppedPreviewUrl: uncroppedAdjustedPreviewUrl,
        selectedImage,
        originalSize,
        previewSize,
      };
    } else {
      cachedEditStateRef.current = null;
    }
  }, [
    selectedImage,
    adjustments,
    histogram,
    waveform,
    finalPreviewUrl,
    uncroppedAdjustedPreviewUrl,
    originalSize,
    previewSize,
    isWgpuActive,
    cachedEditStateRef,
  ]);
}
