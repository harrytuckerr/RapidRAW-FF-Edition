import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface LookBrowserEntry {
  uuid: string;
  name: string;
  group: string;
  requiredBaseProfile: string;
  filePath: string;
  pairing: PairingState;
}

export type PairingState =
  | { state: 'satisfied'; base: string }
  | { state: 'missing_base'; required_name: string; camera: string; installed_for_camera: string[] }
  | { state: 'no_profiles_for_camera'; camera: string };

export interface ProfileEntryView {
  id: string;
  filePath: string;
  uniqueCameraModel: string;
  profileName: string;
  source: string;
}

export interface CameraProfileGroup {
  profileName: string;
  entries: ProfileEntryView[];
}

export interface ProfileBrowserModel {
  camera: string | null;
  cameraProfiles: CameraProfileGroup[];
  looks: LookBrowserEntry[];
}

export interface ImportFailure {
  path: string;
  reason: string;
}

export interface ImportResult {
  imported: number;
  failed: ImportFailure[];
}

export function useProfiles(imagePath: string | null) {
  const [browserModel, setBrowserModel] = useState<ProfileBrowserModel | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activePathRef = useRef(imagePath);
  const requestRef = useRef(0);
  const [modelPath, setModelPath] = useState<string | null>(null);

  const fetchProfiles = useCallback(async (path: string) => {
    const request = ++requestRef.current;
    if (!path) {
      setBrowserModel(null);
      setError(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const model: ProfileBrowserModel = await invoke('list_profiles_for_image', { path });
      if (request !== requestRef.current || path !== activePathRef.current) return;
      setBrowserModel(model);
      setModelPath(path);
    } catch (err) {
      if (request !== requestRef.current || path !== activePathRef.current) return;
      console.error('Failed to list profiles:', err);
      setError(String(err));
      setBrowserModel(null);
    } finally {
      if (request === requestRef.current) setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    activePathRef.current = imagePath;
    setBrowserModel(null);
    setModelPath(null);
    setError(null);
    if (imagePath) {
      void fetchProfiles(imagePath);
    } else {
      setIsLoading(false);
    }
    return () => {
      ++requestRef.current;
      activePathRef.current = null;
    };
  }, [imagePath, fetchProfiles]);

  const importProfiles = useCallback(async (paths: string[]): Promise<ImportResult> => {
    try {
      const result: ImportResult = await invoke('import_profiles', { sourcePaths: paths });
      // Re-fetch after import
      if (activePathRef.current) {
        await fetchProfiles(activePathRef.current);
      }
      return result;
    } catch (err) {
      console.error('Failed to import profiles:', err);
      throw err;
    }
  }, [fetchProfiles]);

  const removeProfile = useCallback(async (id: string): Promise<void> => {
    try {
      await invoke('remove_profile', { id });
      if (activePathRef.current) {
        await fetchProfiles(activePathRef.current);
      }
    } catch (err) {
      console.error('Failed to remove profile:', err);
      throw err;
    }
  }, [fetchProfiles]);

  const rescanProfiles = useCallback(async (): Promise<void> => {
    try {
      await invoke('rescan_profiles');
      if (activePathRef.current) {
        await fetchProfiles(activePathRef.current);
      }
    } catch (err) {
      console.error('Failed to rescan profiles:', err);
      throw err;
    }
  }, [fetchProfiles]);

  const resolvePairingMessage = useCallback((entry: LookBrowserEntry): string | null => {
    const state = entry.pairing;
    if (state.state === 'satisfied') return null;
    if (state.state === 'missing_base') {
      const installedList = state.installed_for_camera.length > 0
        ? ` Installed profiles for ${state.camera}: ${state.installed_for_camera.join(', ')}.`
        : ` No profiles are installed for ${state.camera}.`;
      return `'${entry.name}' requires the base profile '${state.required_name}' for ${state.camera}, which is not installed.${installedList}`;
    }
    if (state.state === 'no_profiles_for_camera') {
      return `No camera profiles are installed for ${state.camera}. Install a base DCP for this camera to use Looks.`;
    }
    return null;
  }, []);

  return {
    browserModel: modelPath === imagePath ? browserModel : null,
    isLoading,
    error,
    fetchProfiles,
    importProfiles,
    removeProfile,
    rescanProfiles,
    resolvePairingMessage,
  };
}
