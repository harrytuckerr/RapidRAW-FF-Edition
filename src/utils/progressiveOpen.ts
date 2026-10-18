export interface OpenKey {
  ticket: string;
  generation: string;
}

export const SOURCE_RENDER_MODES = ['AccurateXtrans1Encoded', 'Legacy'] as const;
export type SourceRenderMode = (typeof SOURCE_RENDER_MODES)[number];
export type SourceRenderPolicy = 'locked' | 'encoded' | 'legacy-native';

export type ProgressiveOpenPhase = 'Pending' | 'Refining' | 'Accurate' | 'Failed' | 'Cancelled';

export interface ProgressiveOpenState {
  ticket: string;
  path: string;
  openKey: OpenKey | null;
  phase: ProgressiveOpenPhase;
  saveReady: boolean;
  placeholderUrl: string | null;
  placeholderUsed: boolean;
  error: string | null;
}

export interface PlaceholderReady {
  schemaVersion: 1;
  openKey: OpenKey;
  width: number;
  height: number;
  mime: 'image/jpeg';
  dataBase64: string;
}

export function parseSourceRenderMode(value: unknown): SourceRenderMode | null {
  return value === 'AccurateXtrans1Encoded' || value === 'Legacy' ? value : null;
}

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const DECIMAL_PATTERN = /^(?:0|[1-9][0-9]*)$/;
const BASE64_PATTERN = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const MAX_GENERATION = 18446744073709551615n;
export const MAX_PLACEHOLDER_BYTES = 2 * 1024 * 1024;
export const MAX_PLACEHOLDER_EVENT_BYTES = 3 * 1024 * 1024;

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function exactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && actual.every((key, index) => key === [...expected].sort()[index]);
}

export function isCanonicalUuid(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const match = UUID_PATTERN.exec(value);
  return match !== null && match[0] === value;
}

export function parseOpenKey(value: unknown): OpenKey | null {
  if (!isPlainRecord(value) || !exactKeys(value, ['ticket', 'generation'])) return null;
  const ticket = value.ticket;
  const generation = value.generation;
  if (!isCanonicalUuid(ticket) || typeof generation !== 'string') return null;
  const match = DECIMAL_PATTERN.exec(generation);
  if (!match || match[0] !== generation) return null;
  try {
    if (BigInt(generation) > MAX_GENERATION) return null;
  } catch {
    return null;
  }
  return { ticket, generation };
}

export function sameOpenKey(left: unknown, right: unknown): boolean {
  const a = parseOpenKey(left);
  const b = parseOpenKey(right);
  return Boolean(a && b && a.ticket === b.ticket && a.generation === b.generation);
}

export function createOpenTicket(): string {
  const cryptoApi = globalThis.crypto;
  if (!cryptoApi?.randomUUID) throw new Error('Secure UUID generation is unavailable.');
  const ticket = cryptoApi.randomUUID();
  if (!isCanonicalUuid(ticket)) throw new Error('Secure UUID generation returned a non-canonical ticket.');
  return ticket;
}

export function isProgressiveMutationBlocked(state: {
  progressiveOpen?: ProgressiveOpenState | null;
  pendingOpenIntent?: { ticket: string; path: string } | null;
  selectedImage?: { openTicket?: string; isReady?: boolean } | null;
}): boolean {
  if (state.pendingOpenIntent) return true;
  if (state.selectedImage?.openTicket && !state.selectedImage.isReady) return true;
  const phase = state.progressiveOpen?.phase;
  return phase === 'Pending' || phase === 'Refining';
}

export function clampEditorTransformForProgressiveFit(
  progressiveLocked: boolean,
  x: number,
  y: number,
  scale: number,
): { x: number; y: number; scale: number } {
  return progressiveLocked ? { x: 0, y: 0, scale: 1 } : { x, y, scale };
}

export function currentAccurateOpenKey(state: {
  selectedImage: {
    path: string;
    openTicket?: string;
    openKey?: OpenKey;
    renderMode?: SourceRenderMode;
    isReady: boolean;
  } | null;
  progressiveOpen?: ProgressiveOpenState | null;
  pendingOpenIntent?: { ticket: string; path: string } | null;
}): OpenKey | undefined {
  const image = state.selectedImage;
  if (!image?.isReady || image.renderMode !== 'AccurateXtrans1Encoded' || !image.openTicket || !image.openKey)
    return undefined;
  const progressive = state.progressiveOpen;
  if (state.pendingOpenIntent) return undefined;
  if (
    progressive?.phase !== 'Accurate' ||
    progressive.path !== image.path ||
    progressive.ticket !== image.openTicket ||
    !sameOpenKey(progressive.openKey, image.openKey)
  ) {
    return undefined;
  }
  return image.openKey;
}

export function sourceRenderPolicy(state: {
  selectedImage: {
    path: string;
    openTicket?: string;
    openKey?: OpenKey;
    renderMode?: SourceRenderMode;
    isReady: boolean;
  } | null;
  progressiveOpen?: ProgressiveOpenState | null;
  pendingOpenIntent?: { ticket: string; path: string } | null;
}): SourceRenderPolicy {
  const image = state.selectedImage;
  if (!image?.isReady || state.pendingOpenIntent) return 'locked';

  if (image.renderMode === 'AccurateXtrans1Encoded') {
    return currentAccurateOpenKey(state) ? 'encoded' : 'locked';
  }

  if (image.renderMode !== undefined && image.renderMode !== 'Legacy') return 'locked';
  if (image.openTicket || image.openKey) return 'locked';

  const progressive = state.progressiveOpen;
  if (
    progressive &&
    progressive.path === image.path &&
    (progressive.phase === 'Pending' || progressive.phase === 'Refining')
  )
    return 'locked';

  // Unkeyed sources without a renderMode are the established legacy route.
  return 'legacy-native';
}

export function isCurrentOpenKey(
  key: unknown,
  state: {
    selectedImage: {
      path: string;
      openTicket?: string;
      openKey?: OpenKey;
      renderMode?: SourceRenderMode;
      isReady: boolean;
    } | null;
    progressiveOpen?: ProgressiveOpenState | null;
    pendingOpenIntent?: { ticket: string; path: string } | null;
  },
  path: string,
  requireAccurate = true,
): boolean {
  const parsed = parseOpenKey(key);
  const image = state.selectedImage;
  const progressive = state.progressiveOpen;
  if (state.pendingOpenIntent && state.pendingOpenIntent.ticket !== parsed?.ticket) return false;
  if (!parsed || !image || image.path !== path || image.openTicket !== parsed.ticket) return false;
  if (requireAccurate) {
    return Boolean(
      image.isReady &&
      image.renderMode === 'AccurateXtrans1Encoded' &&
      sameOpenKey(image.openKey, parsed) &&
      progressive?.phase === 'Accurate' &&
      progressive.path === path &&
      progressive.ticket === parsed.ticket &&
      sameOpenKey(progressive.openKey, parsed),
    );
  }
  return Boolean(
    progressive &&
    progressive.path === path &&
    progressive.ticket === parsed.ticket &&
    (progressive.openKey === null || sameOpenKey(progressive.openKey, parsed)) &&
    (progressive.phase === 'Pending' || progressive.phase === 'Refining'),
  );
}

function base64ByteLength(value: string): number {
  const match = BASE64_PATTERN.exec(value);
  if (value.length === 0 || value.length % 4 !== 0 || !match || match[0] !== value) return -1;
  const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0;
  return (value.length / 4) * 3 - padding;
}

export function validatePlaceholderReady(value: unknown, fitLimit = 1920): PlaceholderReady | null {
  if (!isPlainRecord(value) || !exactKeys(value, ['schemaVersion', 'openKey', 'width', 'height', 'mime', 'dataBase64']))
    return null;
  if (value.schemaVersion !== 1 || value.mime !== 'image/jpeg' || typeof value.dataBase64 !== 'string') return null;
  const key = parseOpenKey(value.openKey);
  if (!key) return null;
  const width = value.width;
  const height = value.height;
  const cap = Number.isInteger(fitLimit) && fitLimit > 0 ? Math.min(1920, fitLimit) : 1920;
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width! <= 0 ||
    height! <= 0 ||
    width! > cap ||
    height! > cap
  )
    return null;
  if (base64ByteLength(value.dataBase64) < 4 || base64ByteLength(value.dataBase64) > MAX_PLACEHOLDER_BYTES) return null;
  try {
    const serializedBytes = new TextEncoder().encode(JSON.stringify(value)).byteLength;
    if (serializedBytes > MAX_PLACEHOLDER_EVENT_BYTES) return null;
  } catch {
    return null;
  }
  return {
    schemaVersion: 1,
    openKey: key,
    width: width as number,
    height: height as number,
    mime: 'image/jpeg',
    dataBase64: value.dataBase64,
  };
}

export function decodeJpegBase64(value: string): Uint8Array | null {
  const byteLength = base64ByteLength(value);
  if (byteLength < 4 || byteLength > MAX_PLACEHOLDER_BYTES) return null;
  try {
    const binary = globalThis.atob(value);
    if (binary.length !== byteLength) return null;
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
    if (bytes[0] !== 0xff || bytes[1] !== 0xd8 || bytes[byteLength - 2] !== 0xff || bytes[byteLength - 1] !== 0xd9)
      return null;
    return bytes;
  } catch {
    return null;
  }
}

interface Flight {
  subscribers: number;
  cancelled: boolean;
  terminal: boolean;
  key: OpenKey | null;
  cancelIssued: boolean;
  placeholderCleanup: (() => void) | null;
  promise: Promise<void>;
}

const openFlights = new Map<string, Flight>();
let admissionQueue: Promise<unknown> = Promise.resolve();

export function serializeOpenAdmission<T>(operation: () => Promise<T>): Promise<T> {
  const next = admissionQueue.then(operation, operation);
  admissionQueue = next.then(
    () => undefined,
    () => undefined,
  );
  return next;
}

export interface OpenFlightLease {
  promise: Promise<void>;
  isCancelled(): boolean;
  isTerminal(): boolean;
  setKey(key: OpenKey): void;
  registerPlaceholderCleanup(cleanup: () => void): boolean;
  clearPlaceholderCleanup(cleanup: () => void): void;
  markTerminal(): void;
  release(): void;
}

export function acquireOpenFlight(
  ticket: string,
  run: (
    flight: Pick<
      OpenFlightLease,
      | 'isCancelled'
      | 'isTerminal'
      | 'setKey'
      | 'registerPlaceholderCleanup'
      | 'clearPlaceholderCleanup'
      | 'markTerminal'
    >,
  ) => Promise<void>,
  cancel: (key: OpenKey) => void,
  onConfirmedCancel?: (key: OpenKey | null) => void,
): OpenFlightLease {
  let flight = openFlights.get(ticket);
  if (flight?.cancelled) {
    if (openFlights.get(ticket) === flight) openFlights.delete(ticket);
    flight = undefined;
  }
  if (!flight) {
    const created: Flight = {
      subscribers: 0,
      cancelled: false,
      terminal: false,
      key: null,
      cancelIssued: false,
      placeholderCleanup: null,
      promise: Promise.resolve(),
    };
    const disposePlaceholder = () => {
      const cleanup = created.placeholderCleanup;
      if (!cleanup) return;
      created.placeholderCleanup = null;
      cleanup();
    };
    const control = {
      isCancelled: () => created.cancelled,
      isTerminal: () => created.terminal,
      setKey: (key: OpenKey) => {
        created.key = key;
      },
      registerPlaceholderCleanup: (cleanup: () => void) => {
        if (created.cancelled || created.terminal || created.placeholderCleanup) return false;
        created.placeholderCleanup = cleanup;
        return true;
      },
      clearPlaceholderCleanup: (cleanup: () => void) => {
        if (created.placeholderCleanup === cleanup) created.placeholderCleanup = null;
      },
      markTerminal: () => {
        created.terminal = true;
        disposePlaceholder();
      },
    };
    created.promise = Promise.resolve()
      .then(() => run(control))
      .catch((error) => {
        throw error;
      });
    flight = created;
    openFlights.set(ticket, created);
    void created.promise
      .finally(() => {
        if (openFlights.get(ticket) === created && created.subscribers === 0) openFlights.delete(ticket);
      })
      .catch(() => {});
  }
  const current = flight;
  current.subscribers++;
  let released = false;
  const isCancelled = () => current.cancelled;
  const disposePlaceholder = () => {
    const cleanup = current.placeholderCleanup;
    if (!cleanup) return;
    current.placeholderCleanup = null;
    cleanup();
  };
  return {
    promise: current.promise,
    isCancelled,
    isTerminal: () => current.terminal,
    setKey: (key) => {
      current.key = key;
    },
    registerPlaceholderCleanup: (cleanup) => {
      if (current.cancelled || current.terminal || current.placeholderCleanup) return false;
      current.placeholderCleanup = cleanup;
      return true;
    },
    clearPlaceholderCleanup: (cleanup) => {
      if (current.placeholderCleanup === cleanup) current.placeholderCleanup = null;
    },
    markTerminal: () => {
      current.terminal = true;
      disposePlaceholder();
    },
    release: () => {
      if (released) return;
      released = true;
      current.subscribers = Math.max(0, current.subscribers - 1);
      if (current.subscribers !== 0) return;
      queueMicrotask(() => {
        if (current.subscribers !== 0) return;
        if (current.terminal) {
          if (openFlights.get(ticket) === current) openFlights.delete(ticket);
          return;
        }
        if (current.cancelled) return;
        current.cancelled = true;
        disposePlaceholder();
        if (openFlights.get(ticket) === current) openFlights.delete(ticket);
        if (current.key && !current.cancelIssued) {
          current.cancelIssued = true;
          cancel(current.key);
        }
        onConfirmedCancel?.(current.key);
      });
    },
  };
}
