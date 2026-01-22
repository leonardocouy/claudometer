import type {
  ClaudeOrganization,
  CodexUsageSource,
  UsageSnapshotBundle,
  UsageSource,
} from './types.ts';

export const ipcChannels = {
  settings: {
    getState: 'settings:getState',
    save: 'settings:save',
    forgetKey: 'settings:forgetKey',
    forgetClaudeKey: 'settings:forgetClaudeKey',
    refreshNow: 'settings:refreshNow',
  },
  events: {
    snapshotUpdated: 'snapshot:updated',
  },
} as const;

export type SettingsState = {
  trackClaudeEnabled: boolean;
  trackCodexEnabled: boolean;
  usageSource: UsageSource;
  rememberSessionKey: boolean;
  codexUsageSource: CodexUsageSource;
  refreshIntervalSeconds: number;
  notifyOnUsageReset: boolean;
  autostartEnabled: boolean;
  checkUpdatesOnStartup: boolean;
  organizations: ClaudeOrganization[];
  selectedOrganizationId?: string;
  latestSnapshot: UsageSnapshotBundle | null;
  keyringAvailable: boolean;
};

export type SaveSettingsPayload = {
  trackClaudeEnabled: boolean;
  trackCodexEnabled: boolean;
  sessionKey?: string;
  rememberSessionKey: boolean;
  codexUsageSource: CodexUsageSource;
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

export type SnapshotUpdatedHandler = (snapshot: UsageSnapshotBundle | null) => void;
export type Unsubscribe = () => void;

export type RendererApi = {
  settings: {
    getState: () => Promise<SettingsState>;
    save: (payload: SaveSettingsPayload) => Promise<IpcResult<null>>;
    forgetKey: () => Promise<IpcResult<null>>;
    forgetClaudeKey: () => Promise<IpcResult<null>>;
    refreshNow: () => Promise<IpcResult<null>>;
    onSnapshotUpdated: (handler: SnapshotUpdatedHandler) => Unsubscribe;
  };
  openExternal: (url: string) => Promise<void>;
};
