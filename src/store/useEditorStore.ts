import { create } from 'zustand';
import { Adjustments, INITIAL_ADJUSTMENTS, MaskContainer, AiPatch } from '../utils/adjustments';
import { SelectedImage, WaveformData, BrushSettings } from '../components/ui/AppProperties';
import { ChannelConfig } from '../components/adjustments/Curves';
import { ImageDimensions } from '../hooks/useImageRenderSize';
import { ToolType } from '../components/panel/right/Masks';
import { OverlayMode } from '../components/panel/right/CropPanel';
import { isProgressiveMutationBlocked, parseSourceRenderMode } from '../utils/progressiveOpen';
import type { OpenKey, ProgressiveOpenState, SourceRenderMode } from '../utils/progressiveOpen';
import type { AppSettings } from '../components/ui/AppProperties';

type RenderedSelectedImage = SelectedImage & { renderMode?: SourceRenderMode };

/** A proof for the existing final Blob, never another pixel cache or saved setting. */
export interface FinalPreviewProof {
  url: string;
  source: NonNullable<RenderedSelectedImage>;
  openKey: OpenKey | null;
  adjustments: Adjustments;
  settings: AppSettings | null;
  jobId: number;
  isCurrent(): boolean;
}

interface InteractivePatch {
  url: string;
  normX: number;
  normY: number;
  normW: number;
  normH: number;
}

interface BaseRenderSize extends ImageDimensions {
  containerHeight: number;
  containerWidth: number;
  offsetX: number;
  offsetY: number;
}

interface EditorState {
  // Core Image & Adjustments
  selectedImage: RenderedSelectedImage | null;
  progressiveOpen: ProgressiveOpenState | null;
  pendingOpenIntent: { ticket: string; path: string } | null;
  adjustments: Adjustments;
  previewOverride: Adjustments | null;

  // History State
  history: Adjustments[];
  historyIndex: number;

  // Previews & Overlays
  finalPreviewUrl: string | null;
  finalPreviewProof: FinalPreviewProof | null;
  autoStraightenBusy: boolean;
  uncroppedAdjustedPreviewUrl: string | null;
  interactivePatch: InteractivePatch | null;
  showOriginal: boolean;

  // Analytics
  histogram: ChannelConfig | null;
  waveform: WaveformData | null;
  isWaveformVisible: boolean;
  activeWaveformChannel: string;
  waveformHeight: number;

  // Interaction State
  isSliderDragging: boolean;
  zoom: number;
  displaySize: ImageDimensions;
  previewSize: ImageDimensions;
  baseRenderSize: BaseRenderSize;
  originalSize: ImageDimensions;

  // Tools State
  isRotationActive: boolean;
  overlayMode: OverlayMode;
  overlayRotation: number;
  isStraightenActive: boolean;
  isWbPickerActive: boolean;
  isPointColourPickerActive: boolean;
  isRelightPickerActive: boolean;
  activeRelightLightId: string | null;
  isGuidedPerspectiveActive: boolean;
  liveRotation: number | null;
  brushSettings: BrushSettings | null;

  // Masks & AI
  activeMaskContainerId: string | null;
  activeMaskId: string | null;
  activeAiPatchContainerId: string | null;
  activeAiSubMaskId: string | null;
  isMaskControlHovered: boolean;
  isGeneratingAiMask: boolean;
  isGeneratingAi: boolean;
  isAIConnectorConnected: boolean;
  hasRenderedFirstFrame: boolean;
  patchesSentToBackend: Set<string>;

  // Clipboard
  copiedSectionAdjustments: any | null;
  copiedMask: MaskContainer | null;
  copiedAdjustments: Adjustments | null;

  // Actions
  setEditor: (updater: Partial<EditorState> | ((state: EditorState) => Partial<EditorState>)) => void;
  pushHistory: (newAdjustments: Adjustments) => void;
  undo: () => void;
  redo: () => void;
  resetHistory: (initialState: Adjustments) => void;
  goToHistoryIndex: (index: number) => void;
  prepareProgressiveIntent: (ticket: string, path: string) => void;
  activateProgressiveIntent: (ticket: string, path: string, thumbnailUrl: string, groupId?: string | null) => boolean;
  failProgressiveIntent: (ticket: string) => boolean;
  acceptProgressiveAdmission: (
    ticket: string,
    path: string,
    openKey: OpenKey,
    metadata: any,
    adjustments: Adjustments,
  ) => boolean;
  claimProgressivePlaceholder: (ticket: string, path: string, openKey: OpenKey) => boolean;
  setProgressivePlaceholder: (ticket: string, path: string, openKey: OpenKey, url: string) => boolean;
  completeProgressiveLoad: (ticket: string, path: string, openKey: OpenKey, result: any) => boolean;
  failProgressiveOpen: (ticket: string, path: string, openKey: OpenKey | null, error: string) => boolean;
  cancelProgressiveOpen: (ticket?: string) => boolean;
}

export const useEditorStore = create<EditorState>((set) => ({
  selectedImage: null,
  progressiveOpen: null,
  pendingOpenIntent: null,
  adjustments: INITIAL_ADJUSTMENTS,
  previewOverride: null,
  history: [INITIAL_ADJUSTMENTS],
  historyIndex: 0,

  finalPreviewUrl: null,
  finalPreviewProof: null,
  autoStraightenBusy: false,
  uncroppedAdjustedPreviewUrl: null,
  showOriginal: false,
  histogram: null,
  waveform: null,
  isWaveformVisible: false,
  activeWaveformChannel: 'luma',
  waveformHeight: 220,

  isSliderDragging: false,
  interactivePatch: null,
  activeMaskContainerId: null,
  activeMaskId: null,
  activeAiPatchContainerId: null,
  activeAiSubMaskId: null,

  zoom: 1,
  displaySize: { width: 0, height: 0 },
  previewSize: { width: 0, height: 0 },
  baseRenderSize: { width: 0, height: 0, offsetX: 0, offsetY: 0, containerWidth: 0, containerHeight: 0 },
  originalSize: { width: 0, height: 0 },

  isRotationActive: false,
  overlayMode: 'thirds',
  overlayRotation: 0,
  isStraightenActive: false,
  isWbPickerActive: false,
  isPointColourPickerActive: false,
  isRelightPickerActive: false,
  activeRelightLightId: null,
  isGuidedPerspectiveActive: false,
  liveRotation: null,

  copiedSectionAdjustments: null,
  copiedMask: null,
  brushSettings: { size: 50, feather: 50, tool: ToolType.Brush },
  copiedAdjustments: null,

  isGeneratingAiMask: false,
  isAIConnectorConnected: false,
  isGeneratingAi: false,
  isMaskControlHovered: false,
  hasRenderedFirstFrame: false,
  patchesSentToBackend: new Set<string>(),

  setEditor: (updater) =>
    set((state) => {
      const update = typeof updater === 'function' ? updater(state) : updater;
      if (update === state) return state;
      const next = { ...update };
      if (
        next.finalPreviewProof === undefined &&
        ((next.selectedImage !== undefined && next.selectedImage !== state.selectedImage) ||
          (next.adjustments !== undefined && next.adjustments !== state.adjustments) ||
          (next.finalPreviewUrl !== undefined && next.finalPreviewUrl !== state.finalPreviewUrl) ||
          next.pendingOpenIntent ||
          next.previewOverride ||
          next.showOriginal ||
          next.isSliderDragging ||
          next.isRotationActive ||
          next.liveRotation != null)
      )
        next.finalPreviewProof = null;
      if (next.isPointColourPickerActive) {
        next.isWbPickerActive = false;
        next.isRelightPickerActive = false;
      } else if (
        next.isWbPickerActive ||
        next.isRelightPickerActive ||
        next.pendingOpenIntent ||
        (next.selectedImage !== undefined && next.selectedImage !== state.selectedImage)
      ) {
        next.isPointColourPickerActive = false;
      }
      if (!isProgressiveMutationBlocked(state)) return next;
      const {
        adjustments: _adjustments,
        finalPreviewUrl: _preview,
        finalPreviewProof: _proof,
        uncroppedAdjustedPreviewUrl: _uncropped,
        ...safe
      } = next;
      return safe;
    }),

  pushHistory: (newAdj) =>
    set((state) => {
      if (isProgressiveMutationBlocked(state)) return state;
      const newHistory = state.history.slice(0, state.historyIndex + 1);
      newHistory.push(newAdj);
      if (newHistory.length > 50) newHistory.shift();
      return { history: newHistory, historyIndex: newHistory.length - 1 };
    }),

  undo: () =>
    set((state) => {
      if (isProgressiveMutationBlocked(state)) return state;
      if (state.historyIndex > 0) {
        const newIndex = state.historyIndex - 1;
        return { historyIndex: newIndex, adjustments: state.history[newIndex], finalPreviewProof: null };
      }
      return state;
    }),

  redo: () =>
    set((state) => {
      if (isProgressiveMutationBlocked(state)) return state;
      if (state.historyIndex < state.history.length - 1) {
        const newIndex = state.historyIndex + 1;
        return { historyIndex: newIndex, adjustments: state.history[newIndex], finalPreviewProof: null };
      }
      return state;
    }),

  resetHistory: (initialState) =>
    set((state) =>
      isProgressiveMutationBlocked(state)
        ? state
        : { history: [initialState], historyIndex: 0, adjustments: initialState },
    ),

  goToHistoryIndex: (index) =>
    set((state) => {
      if (isProgressiveMutationBlocked(state)) return state;
      if (index >= 0 && index < state.history.length) {
        return { historyIndex: index, adjustments: state.history[index] };
      }
      return state;
    }),

  prepareProgressiveIntent: (ticket, path) => set({ pendingOpenIntent: { ticket, path }, finalPreviewProof: null }),

  activateProgressiveIntent: (ticket, path, thumbnailUrl, groupId = null) => {
    let accepted = false;
    set((state) => {
      if (state.pendingOpenIntent?.ticket !== ticket || state.pendingOpenIntent.path !== path) return state;
      accepted = true;
      return {
        pendingOpenIntent: null,
        progressiveOpen: {
          ticket,
          path,
          openKey: null,
          phase: 'Pending',
          saveReady: true,
          placeholderUrl: null,
          placeholderUsed: false,
          error: null,
        },
        selectedImage: {
          exif: null,
          group_id: groupId,
          height: 0,
          isRaw: false,
          isReady: false,
          metadata: null,
          openTicket: ticket,
          path,
          thumbnailUrl,
          width: 0,
        },
        originalSize: { width: 0, height: 0 },
        previewSize: { width: 0, height: 0 },
        finalPreviewUrl: null,
        finalPreviewProof: null,
        uncroppedAdjustedPreviewUrl: null,
        histogram: null,
        waveform: null,
        hasRenderedFirstFrame: false,
        showOriginal: false,
        previewOverride: null,
        zoom: 1,
      };
    });
    return accepted;
  },

  failProgressiveIntent: (ticket) => {
    let accepted = false;
    set((state) => {
      if (state.pendingOpenIntent?.ticket !== ticket) return state;
      accepted = true;
      return { pendingOpenIntent: null };
    });
    return accepted;
  },

  acceptProgressiveAdmission: (ticket, path, openKey, metadata, initialAdjustments) => {
    let accepted = false;
    set((state) => {
      const current = state.progressiveOpen;
      if (
        !current ||
        current.ticket !== ticket ||
        current.path !== path ||
        current.phase !== 'Pending' ||
        !current.saveReady ||
        state.selectedImage?.path !== path ||
        state.selectedImage.openTicket !== ticket ||
        openKey.ticket !== ticket
      )
        return state;
      accepted = true;
      return {
        progressiveOpen: { ...current, openKey, error: null },
        selectedImage: { ...state.selectedImage, metadata, openKey },
        adjustments: initialAdjustments,
        history: [initialAdjustments],
        historyIndex: 0,
        showOriginal: false,
        previewOverride: null,
      };
    });
    return accepted;
  },

  setProgressivePlaceholder: (ticket, path, openKey, url) => {
    let accepted = false;
    set((state) => {
      const current = state.progressiveOpen;
      if (
        !current ||
        current.ticket !== ticket ||
        current.path !== path ||
        current.phase !== 'Pending' ||
        !current.placeholderUsed ||
        current.placeholderUrl ||
        !current.openKey ||
        current.openKey.generation !== openKey.generation ||
        current.openKey.ticket !== openKey.ticket ||
        state.selectedImage?.path !== path ||
        state.selectedImage.openTicket !== ticket ||
        state.selectedImage.isReady
      )
        return state;
      accepted = true;
      return {
        progressiveOpen: { ...current, phase: 'Refining', placeholderUrl: url, placeholderUsed: true },
      };
    });
    return accepted;
  },

  claimProgressivePlaceholder: (ticket, path, openKey) => {
    let accepted = false;
    set((state) => {
      const current = state.progressiveOpen;
      if (
        !current ||
        current.ticket !== ticket ||
        current.path !== path ||
        current.phase !== 'Pending' ||
        current.placeholderUsed ||
        !current.openKey ||
        current.openKey.ticket !== openKey.ticket ||
        current.openKey.generation !== openKey.generation ||
        state.selectedImage?.path !== path ||
        state.selectedImage.openTicket !== ticket ||
        state.selectedImage.isReady
      )
        return state;
      accepted = true;
      return { progressiveOpen: { ...current, placeholderUsed: true } };
    });
    return accepted;
  },

  completeProgressiveLoad: (ticket, path, openKey, result) => {
    const renderMode =
      result && typeof result === 'object' && Object.prototype.hasOwnProperty.call(result, 'renderMode')
        ? parseSourceRenderMode(result.renderMode)
        : null;
    if (!renderMode) return false;
    let accepted = false;
    set((state) => {
      const current = state.progressiveOpen;
      if (
        !current ||
        current.ticket !== ticket ||
        current.path !== path ||
        current.openKey?.generation !== openKey.generation ||
        current.openKey.ticket !== openKey.ticket ||
        (current.phase !== 'Pending' && current.phase !== 'Refining') ||
        state.selectedImage?.path !== path ||
        state.selectedImage.openTicket !== ticket ||
        !state.selectedImage.openKey ||
        state.selectedImage.openKey.ticket !== openKey.ticket ||
        state.selectedImage.openKey.generation !== openKey.generation
      )
        return state;
      accepted = true;
      const adjustments =
        !state.adjustments.aspectRatio && !state.adjustments.crop
          ? { ...state.adjustments, aspectRatio: result.width / result.height }
          : state.adjustments;
      const selectedImage: RenderedSelectedImage = {
        ...state.selectedImage,
        asShotWhiteBalance: result.as_shot_white_balance,
        exif: result.exif,
        height: result.height,
        isRaw: result.is_raw,
        isReady: true,
        metadata: state.selectedImage.metadata,
        renderMode,
        width: result.width,
      };
      if (renderMode === 'Legacy') {
        delete selectedImage.openKey;
        delete selectedImage.openTicket;
      } else {
        selectedImage.openKey = openKey;
        selectedImage.openTicket = ticket;
      }
      return {
        progressiveOpen:
          renderMode === 'Legacy' ? null : { ...current, phase: 'Accurate', placeholderUrl: null, error: null },
        selectedImage,
        originalSize: { width: result.width, height: result.height },
        previewSize: result.previewSize,
        adjustments,
        hasRenderedFirstFrame: false,
      };
    });
    return accepted;
  },

  failProgressiveOpen: (ticket, path, openKey, error) => {
    let accepted = false;
    set((state) => {
      const current = state.progressiveOpen;
      if (
        !current ||
        current.ticket !== ticket ||
        current.path !== path ||
        (openKey &&
          (!current.openKey ||
            current.openKey.ticket !== openKey.ticket ||
            current.openKey.generation !== openKey.generation)) ||
        (current.phase !== 'Pending' && current.phase !== 'Refining')
      )
        return state;
      accepted = true;
      return {
        progressiveOpen: { ...current, phase: 'Failed', placeholderUrl: null, error },
        selectedImage:
          state.selectedImage?.openTicket === ticket ? { ...state.selectedImage, isReady: false } : state.selectedImage,
      };
    });
    return accepted;
  },

  cancelProgressiveOpen: (ticket) => {
    let accepted = false;
    set((state) => {
      const current = state.progressiveOpen;
      if (ticket && state.pendingOpenIntent?.ticket === ticket) {
        accepted = true;
        return { pendingOpenIntent: null };
      }
      if (!current || (ticket && current.ticket !== ticket) || current.phase === 'Accurate') return state;
      accepted = true;
      return { progressiveOpen: { ...current, phase: 'Cancelled', placeholderUrl: null } };
    });
    return accepted;
  },
}));
