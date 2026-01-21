import type {
  ClaudeOrganization,
  CodexUsageSource,
  UsageProvider,
  UsageSnapshot,
  UsageSource,
} from './types.ts';

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
  provider: UsageProvider;
  usageSource: UsageSource;
  rememberSessionKey: boolean;
  codexUsageSource: CodexUsageSource;
  rememberCodexCookie: boolean;
  refreshIntervalSeconds: number;
  notifyOnUsageReset: boolean;
  autostartEnabled: boolean;
  checkUpdatesOnStartup: boolean;
  organizations: ClaudeOrganization[];
  selectedOrganizationId?: string;
  latestSnapshot: UsageSnapshot | null;
  keyringAvailable: boolean;
};

export type SaveSettingsPayload = {
  provider: UsageProvider;
  sessionKey?: string;
  rememberSessionKey: boolean;
  codexUsageSource: CodexUsageSource;
  codexCookie?: string;
  rememberCodexCookie: boolean;
  refreshIntervalSeconds: number;
  notifyOnUsageReset: boolean;
  autostartEnabled: boolean;
  checkUpdatesOnStartup: boolean;
  selectedOrganizationId?: string;
  usageSource: UsageSource;
};

export type IpcErrorCode =
  | 'VALIDATION'
  | 'NETWORK'
  | 'UNAUTHORIZED'
  | 'RATE_LIMITED'
  | 'KEYRING'
  | 'UPDATER'
  | 'UNKNOWN';

export type IpcError = { code: IpcErrorCode; message: string };

export type IpcResult<T> = { ok: true; value: T } | { ok: false; error: IpcError };

export type SnapshotUpdatedHandler = (snapshot: UsageSnapshot | null) => void;
export type Unsubscribe = () => void;

export type RendererApi = {
  settings: {
    getState: () => Promise<SettingsState>;
    save: (payload: SaveSettingsPayload) => Promise<IpcResult<null>>;
    forgetKey: () => Promise<IpcResult<null>>;
    refreshNow: () => Promise<IpcResult<null>>;
    onSnapshotUpdated: (handler: SnapshotUpdatedHandler) => Unsubscribe;
  };
  openExternal: (url: string) => Promise<void>;
};
