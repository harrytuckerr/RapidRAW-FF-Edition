import { v4 as uuidv4 } from 'uuid';

// Wire DTOs emitted by the native `import_files` worker. All fields are required.
export interface ImportStart {
  runId: string;
  total: number;
}
export interface ImportProgress {
  runId: string;
  total: number;
  current: number;
  path: string;
}
export interface ImportItemError {
  runId: string;
  total: number;
  failed: number;
  errorMessage: string;
}
export interface ImportComplete {
  runId: string;
  total: number;
  imported: number;
  failed: number;
  errorMessage: string;
}

export const IMPORT_EVENT_NAMES = ['import-start', 'import-progress', 'import-error', 'import-complete'] as const;
export type ImportEventName = (typeof IMPORT_EVENT_NAMES)[number];

export const IMPORT_ALREADY_RUNNING_MESSAGE = 'An import is already running';
export const IMPORT_NOT_READY_MESSAGE = 'Import is not ready yet. Please try again in a moment.';
export const IMPORT_NO_FILES_MESSAGE = 'No files were selected for import.';
export const IMPORT_INVALID_FILES_MESSAGE = 'The selected file list is invalid. Select the files again.';
export const IMPORT_NO_DESTINATION_MESSAGE = 'No destination folder was selected for import.';
export const IMPORT_UNKNOWN_RESULT_MESSAGE =
  'Import result could not be verified; some files may have been imported. Refresh the folder before retrying.';

const MAX_MESSAGE_SCALARS = 1024;
const MAX_PATH_SCALARS = 256;
const RUN_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);
const isCount = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const isBoundedText = (value: unknown, max: number): value is string => {
  if (typeof value !== 'string') return false;
  let count = 0;
  for (const _scalar of value) if (++count > max) return false;
  return true;
};
const isCanonicalRunId = (value: unknown): value is string => typeof value === 'string' && RUN_ID_PATTERN.test(value);
const bounded = (text: string, max: number) => {
  let result = '';
  let count = 0;
  for (const scalar of text) {
    if (count++ === max) break;
    result += scalar;
  }
  return result;
};

interface ActiveRun {
  runId: string;
  total: number;
  listenerGeneration: number;
  maxCurrent: number;
  maxFailed: number;
}

// Module-scoped and shared by every hook instance: one listener generation and
// at most one active import identity.
let listenerGeneration = 0;
let readyGeneration: number | null = null;
let active: ActiveRun | null = null;

export type ImportAdmission =
  | { kind: 'admitted'; runId: string; total: number; listenerGeneration: number }
  | { kind: 'busy' }
  | { kind: 'notReady' }
  | { kind: 'invalid'; message: string };

/** What a hook must do in response to the coordinator. Identity is already retired for terminal effects. */
export type ImportEffect =
  | { kind: 'none' }
  | { kind: 'progress'; current: number; total: number; path: string }
  | { kind: 'itemError'; errorMessage: string }
  | { kind: 'terminal'; runId: string; failed: number; total: number; errorMessage: string }
  | { kind: 'unknownResult'; runId: string; refresh: boolean };

const NONE: ImportEffect = { kind: 'none' };

/** Superseding listeners retires any active run as unknown. */
export function beginImportListenerGeneration(): { generation: number; effect: ImportEffect } {
  const effect: ImportEffect = active ? { kind: 'unknownResult', runId: active.runId, refresh: false } : NONE;
  active = null;
  listenerGeneration += 1;
  readyGeneration = null;
  return { generation: listenerGeneration, effect };
}

/** Marks a generation ready once all four import listeners are registered. */
export function markImportListenersReady(generation: number) {
  if (generation === listenerGeneration) readyGeneration = generation;
}

/**
 * Invalidates the generation if it is still current. A run owned by it is
 * retired as an unknown result (never success); an older cleanup is inert.
 */
export function endImportListenerGeneration(generation: number): ImportEffect {
  if (generation !== listenerGeneration) return NONE;
  listenerGeneration += 1;
  readyGeneration = null;
  if (active && active.listenerGeneration === generation) {
    const { runId } = active;
    active = null;
    return { kind: 'unknownResult', runId, refresh: false };
  }
  return NONE;
}

/** Synchronously admits and reserves a run before any native call, or explains why not. */
export function requestImportAdmission(sourcePaths: unknown, destinationFolder: unknown): ImportAdmission {
  if (active) return { kind: 'busy' };
  if (!Array.isArray(sourcePaths) || sourcePaths.length === 0) {
    return { kind: 'invalid', message: IMPORT_NO_FILES_MESSAGE };
  }
  for (const path of sourcePaths) {
    if (typeof path !== 'string' || path.trim() === '')
      return { kind: 'invalid', message: IMPORT_INVALID_FILES_MESSAGE };
  }
  if (typeof destinationFolder !== 'string' || destinationFolder.trim() === '') {
    return { kind: 'invalid', message: IMPORT_NO_DESTINATION_MESSAGE };
  }
  if (readyGeneration === null || readyGeneration !== listenerGeneration) return { kind: 'notReady' };

  const runId = uuidv4();
  active = {
    runId,
    total: sourcePaths.length,
    listenerGeneration: readyGeneration,
    maxCurrent: 0,
    maxFailed: 0,
  };
  return { kind: 'admitted', runId, total: sourcePaths.length, listenerGeneration: readyGeneration };
}

/** Handles a rejected native invoke; only the exact still-active token may report it. */
export function failImportInvocation(
  runId: string,
  generation: number,
  error: unknown,
): { errorMessage: string } | null {
  if (!active || active.runId !== runId || active.listenerGeneration !== generation) return null;
  active = null;
  return { errorMessage: bounded(`Failed to start import: ${String(error)}`, MAX_MESSAGE_SCALARS) };
}

function failClosed(run: ActiveRun): ImportEffect {
  active = null;
  return { kind: 'unknownResult', runId: run.runId, refresh: true };
}

/** Validates one native import event for the current generation and active run. */
export function consumeImportEvent(generation: number, name: ImportEventName, payload: unknown): ImportEffect {
  const run = active;
  if (generation !== listenerGeneration || !run || run.listenerGeneration !== generation) return NONE;
  // Foreign or unattributable events never touch the active run.
  if (!isRecord(payload) || !isCanonicalRunId(payload.runId) || payload.runId !== run.runId) return NONE;

  const prototype = Object.getPrototypeOf(payload);
  if (prototype !== Object.prototype && prototype !== null) return failClosed(run);

  const { total } = payload;
  if (!isCount(total) || total === 0 || total !== run.total) return failClosed(run);

  switch (name) {
    case 'import-start':
      return NONE;

    case 'import-progress': {
      const { current, path } = payload;
      if (!isCount(current) || current > total || !isBoundedText(path, MAX_PATH_SCALARS)) return failClosed(run);
      if (current < run.maxCurrent) return NONE;
      run.maxCurrent = current;
      return { kind: 'progress', current, total, path };
    }

    case 'import-error': {
      const { failed, errorMessage } = payload;
      if (
        !isCount(failed) ||
        failed < 1 ||
        failed > total ||
        !isBoundedText(errorMessage, MAX_MESSAGE_SCALARS) ||
        errorMessage.trim() === ''
      ) {
        return failClosed(run);
      }
      run.maxFailed = Math.max(run.maxFailed, failed);
      return { kind: 'itemError', errorMessage };
    }

    case 'import-complete': {
      const { imported, failed, errorMessage } = payload;
      const valid =
        isCount(imported) &&
        isCount(failed) &&
        imported <= total &&
        failed <= total &&
        imported + failed === total &&
        failed >= run.maxFailed &&
        isBoundedText(errorMessage, MAX_MESSAGE_SCALARS) &&
        (failed === 0 ? errorMessage === '' : errorMessage.trim() !== '');
      if (!valid) return failClosed(run);
      active = null;
      return { kind: 'terminal', runId: run.runId, failed, total, errorMessage };
    }

    default:
      return NONE;
  }
}
