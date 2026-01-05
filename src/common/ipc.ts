import type { ClaudeOrganization, ClaudeUsageSnapshot } from './types.ts';

export const ipcChannels = {
  settings: {
    getState: 'settings:getState',
    save: 'settings:save',
    forgetKey: 'settings:forgetKey',
    refreshNow: 'settings:refreshNow',
  },
  events: {
    snapshotUpdated: 'snapshot:updated',
  },
} as const;

export type SettingsState = {
  rememberSessionKey: boolean;
  refreshIntervalSeconds: number;
  organizations: ClaudeOrganization[];
  selectedOrganizationId?: string;
  latestSnapshot: ClaudeUsageSnapshot | null;
  encryptionAvailable: boolean;
};

export type SaveSettingsPayload = {
  sessionKey: string;
  rememberSessionKey: boolean;
  refreshIntervalSeconds: number;
  selectedOrganizationId?: string;
};

export type IpcErrorCode = 'VALIDATION' | 'NETWORK' | 'UNAUTHORIZED' | 'RATE_LIMITED' | 'UNKNOWN';

export type IpcError = { code: IpcErrorCode; message: string };

export type IpcResult<T> = { ok: true; value: T } | { ok: false; error: IpcError };

export type SnapshotUpdatedHandler = (snapshot: ClaudeUsageSnapshot | null) => void;
export type Unsubscribe = () => void;

export type RendererApi = {
  settings: {
    getState: () => Promise<SettingsState>;
    save: (payload: SaveSettingsPayload) => Promise<IpcResult<null>>;
    forgetKey: () => Promise<IpcResult<null>>;
    refreshNow: () => Promise<IpcResult<null>>;
    onSnapshotUpdated: (handler: SnapshotUpdatedHandler) => Unsubscribe;
  };
};
