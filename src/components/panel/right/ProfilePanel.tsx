import { useState, useCallback, useEffect, useRef } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { RotateCcw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import Slider from '../../ui/Slider';
import ProfileBrowser from '../../ui/ProfileBrowser';
import type { Adjustments, ProfileSelection } from '../../../utils/adjustments';
import { useProfiles, ImportFailure } from '../../../hooks/useProfiles';
import Text from '../../ui/Text';
import Button from '../../ui/Button';
import Dropdown from '../../ui/Dropdown';
import { useUIStore } from '../../../store/useUIStore';
import { useEditorStore } from '../../../store/useEditorStore';
import { useEditorActions } from '../../../hooks/useEditorActions';
import { Panel } from '../../ui/AppProperties';
import { TextColors, TextVariants, TextWeights } from '../../../types/typography';

interface ProfilePanelProps {
  adjustments: Adjustments;
  setAdjustments: (value: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)) => void;
  imagePath: string | null;
  onDragStateChange?: (isDragging: boolean) => void;
}

export default function ProfilePanel({ adjustments, setAdjustments, imagePath, onDragStateChange }: ProfilePanelProps) {
  const { t } = useTranslation();
  const { browserModel, importProfiles, isLoading, error } = useProfiles(imagePath);
  const setPanel = useUIStore((state) => state.setPanel);
  const [isImporting, setIsImporting] = useState(false);
  const importMenuRef = useRef<HTMLDivElement>(null);
  const importRequestRef = useRef<{ kind: 'dcp' | 'xmp'; started: boolean } | null>(null);
  const restoreImportFocusRef = useRef(false);
  const [showPresetGuidance, setShowPresetGuidance] = useState(false);

  const profile: ProfileSelection = adjustments.profile || { base: null, look: null, amount: 100 };
  const hasLook = !!profile.look;
  const hasAnyProfile = !!(profile.base || profile.look);
  const baseName = browserModel?.cameraProfiles
    .flatMap((group) => group.entries)
    .find((entry) => entry.id === profile.base)?.profileName;
  const lookName = browserModel?.looks.find((look) => look.uuid === profile.look)?.name;

  const handleSelectBase = useCallback(
    (id: string) => {
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        profile: {
          base: id,
          look: null,
          amount: 100,
        },
      }));
    },
    [setAdjustments],
  );

  const handleSelectLook = useCallback(
    (uuid: string, baseId: string) => {
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        profile: {
          base: baseId,
          look: uuid,
          amount: prev.profile?.amount ?? 100,
        },
      }));
    },
    [setAdjustments],
  );

  const handleClear = useCallback(() => {
    setAdjustments((prev: Adjustments) => ({
      ...prev,
      profile: {
        base: null,
        look: null,
        amount: 100,
      },
    }));
  }, [setAdjustments]);

  const handleAmountChange = useCallback(
    (value: number) => {
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        profile: {
          base: prev.profile?.base ?? null,
          look: prev.profile?.look ?? null,
          amount: value,
        },
      }));
    },
    [setAdjustments],
  );

  const handleImport = useCallback(
    async (kind: 'dcp' | 'xmp') => {
      setIsImporting(true);
      setShowPresetGuidance(false);
      try {
        const selected = await open({
          multiple: true,
          title: t(kind === 'dcp' ? 'profiles.importBase' : 'profiles.importLook'),
          filters: [
            {
              name: t(kind === 'dcp' ? 'profiles.baseFilterName' : 'profiles.lookFilterName'),
              extensions: [kind],
            },
          ],
        });

        if (!selected || (Array.isArray(selected) && selected.length === 0)) return;
        const paths = Array.isArray(selected) ? selected : [selected];
        const result = await importProfiles(paths);
        if (result.imported > 0) {
          toast.success(t('profiles.importSuccess', { count: result.imported }));
        }
        if (result.failed.length > 0) {
          const failures = result.failed.map((f: ImportFailure) => `${f.path}: ${f.reason}`).join('\n');
          const guidance = kind === 'xmp' ? `\n${t('profiles.presetImportHint')}` : '';
          setShowPresetGuidance(kind === 'xmp');
          toast.error(`${t('profiles.importFailed')}\n${failures}${guidance}`);
        }
        if (result.imported === 0 && result.failed.length === 0) {
          toast.info(t('profiles.importNone'));
        }
      } catch (err) {
        console.error('Import dialog error:', err);
        toast.error(`${t('profiles.importFailed')}\n${String(err)}`);
      } finally {
        importRequestRef.current = null;
        setIsImporting(false);
      }
    },
    [importProfiles, t],
  );

  const requestImport = useCallback((kind: 'dcp' | 'xmp') => {
    // Guard immediately: two callbacks before React commits still open one picker.
    if (importRequestRef.current) return;
    importRequestRef.current = { kind, started: false };
    restoreImportFocusRef.current = true;
    setIsImporting(true);
  }, []);

  useEffect(() => {
    const request = importRequestRef.current;
    if (isImporting && request && !request.started) {
      // The busy-keyed Dropdown has committed closed before invoking the picker.
      // A repeated effect cannot launch this same request twice.
      request.started = true;
      void handleImport(request.kind);
    } else if (!isImporting && restoreImportFocusRef.current) {
      restoreImportFocusRef.current = false;
      importMenuRef.current?.querySelector<HTMLButtonElement>('button')?.focus();
    }
  }, [isImporting, handleImport]);

  const handleImportKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (isImporting) {
      if (event.key !== 'Tab') event.preventDefault();
      event.stopPropagation();
      return;
    }
    const surface = event.currentTarget;
    const trigger = surface.querySelector<HTMLButtonElement>('button');
    if (event.key === 'Escape') {
      trigger?.focus(); // Dropdown's own Escape handler closes the popup.
    } else if (event.key === ' ') {
      // Preserve native button activation instead of Dropdown's type-to-search.
      event.stopPropagation();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      event.stopPropagation();
      const direction = event.key === 'ArrowDown' ? 1 : -1;
      const focusOption = () => {
        const options = Array.from(surface.querySelectorAll<HTMLButtonElement>('button[role="option"]:not(:disabled)'));
        if (!options.length) return;
        const index = options.indexOf(document.activeElement as HTMLButtonElement);
        const next =
          index < 0 ? (direction > 0 ? 0 : options.length - 1) : (index + direction + options.length) % options.length;
        options[next].focus();
      };
      if (trigger?.getAttribute('aria-expanded') === 'false') {
        trigger.click();
        requestAnimationFrame(() => {
          if (importMenuRef.current === surface && !importRequestRef.current) focusOption();
        });
      } else {
        focusOption();
      }
    }
  };

  return (
    <div className="mb-2">
      <div className="flex justify-between items-center mb-1">
        <Text variant={TextVariants.title} weight={TextWeights.normal}>
          {t('profiles.title')}
        </Text>
        {hasAnyProfile && (
          <button
            type="button"
            className="p-2.5 rounded-full hover:bg-surface transition-colors min-h-[44px] min-w-[44px] flex items-center justify-center focus-visible:outline-2 focus-visible:outline-accent"
            onClick={handleClear}
            aria-label={t('profiles.resetTooltip')}
            data-tooltip={t('profiles.resetTooltip')}
          >
            <RotateCcw size={16} />
          </button>
        )}
      </div>
      <Text variant={TextVariants.small} color={TextColors.secondary} className="mb-2">
        {t('profiles.importMenuHint')}
      </Text>
      <div
        ref={importMenuRef}
        data-profile-import-menu
        aria-busy={isImporting}
        className="mb-3 [&_button]:min-h-[44px] [&_button:focus-visible]:outline-2 [&_button:focus-visible]:outline-accent"
        onKeyDownCapture={handleImportKeyDown}
      >
        <Dropdown<'dcp' | 'xmp'>
          key={isImporting ? 'importing' : 'idle'}
          value={null}
          placeholder={t('profiles.importMenu')}
          onChange={requestImport}
          disabled={isImporting}
          triggerClassName="bg-surface !mr-0 min-h-[44px] focus-visible:outline-2 focus-visible:outline-accent"
          options={[
            { label: t('profiles.importBase'), value: 'dcp', disabled: isImporting },
            { label: t('profiles.importLook'), value: 'xmp', disabled: isImporting },
          ]}
        />
      </div>
      {showPresetGuidance && (
        <div className="mb-3 space-y-1" role="status">
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {t('profiles.presetImportHint')}
          </Text>
          <Button
            type="button"
            className="bg-surface text-text-primary text-sm min-h-[44px] focus-visible:outline-2 focus-visible:outline-accent hover:bg-card-active"
            onClick={() => setPanel(Panel.Presets)}
          >
            {t('profiles.goToPresets')}
          </Button>
        </div>
      )}
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-1 mb-2 text-sm">
        <dt className="text-text-secondary">{t('profiles.baseLabel')}</dt>
        <dd className="text-text-primary break-words">
          {profile.base ? baseName || t('profiles.unavailableBase') : t('profiles.noBase')}
        </dd>
        <dt className="text-text-secondary">{t('profiles.lookLabel')}</dt>
        <dd className="text-text-primary break-words">
          {profile.look
            ? t('profiles.lookSummary', { name: lookName || t('profiles.unavailableLook'), amount: profile.amount })
            : t('profiles.noLook')}
        </dd>
      </dl>
      {isLoading && <Text variant={TextVariants.small}>{t('profiles.loading')}</Text>}
      {error && (
        <div role="alert">
          <Text variant={TextVariants.small} color={TextColors.error}>
            {error}
          </Text>
        </div>
      )}

      <ProfileBrowser
        model={browserModel}
        activeBase={profile.base}
        activeLook={profile.look}
        onSelectBase={handleSelectBase}
        onSelectLook={handleSelectLook}
        onClear={handleClear}
        className="mb-2"
      />

      {hasLook && (
        <Slider
          label={t('profiles.amount')}
          max={200}
          min={0}
          onChange={(e: React.ChangeEvent<HTMLInputElement> | { target: { value: number | string } }) => {
            const val = parseFloat(String(e.target.value));
            handleAmountChange(val);
          }}
          step={1}
          value={profile.amount}
          defaultValue={100}
          trackClassName="bg-surface"
          onDragStateChange={onDragStateChange}
        />
      )}
    </div>
  );
}

// Same source and guarded history setter previously supplied by ControlsPanel.
export function ProfilesWorkspacePanel() {
  const { t } = useTranslation();
  const selectedImage = useEditorStore((state) => state.selectedImage);
  const adjustments = useEditorStore((state) => state.adjustments);
  const setEditor = useEditorStore((state) => state.setEditor);
  const { setAdjustments } = useEditorActions();
  const onDragStateChange = useCallback(
    (isDragging: boolean) => setEditor({ isSliderDragging: isDragging }),
    [setEditor],
  );

  return (
    <div className="h-full min-h-0 overflow-y-auto p-3">
      {selectedImage ? (
        <ProfilePanel
          adjustments={adjustments}
          setAdjustments={setAdjustments}
          imagePath={selectedImage.path}
          onDragStateChange={onDragStateChange}
        />
      ) : (
        <Text variant={TextVariants.heading} color={TextColors.secondary} className="text-center">
          {t('editor.ai.noImageSelected')}
        </Text>
      )}
    </div>
  );
}
