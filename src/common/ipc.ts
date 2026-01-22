import type {
  IpcResult,
  SaveSettingsPayload,
  SettingsState,
  UsageSnapshotBundle,
} from './generated/ipc-types.ts';

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
