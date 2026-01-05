import { contextBridge, ipcRenderer } from 'electron';
import type {
  IpcResult,
  RendererApi,
  SaveSettingsPayload,
  SettingsState,
  SnapshotUpdatedHandler,
} from '../common/ipc.ts';
import { ipcChannels } from '../common/ipc.ts';
import type { ClaudeUsageSnapshot } from '../common/types.ts';

const api: RendererApi = {
  settings: {
    getState: async () =>
      (await ipcRenderer.invoke(ipcChannels.settings.getState)) as SettingsState,
    save: async (payload: SaveSettingsPayload) =>
      (await ipcRenderer.invoke(ipcChannels.settings.save, payload)) as IpcResult<null>,
    forgetKey: async () =>
      (await ipcRenderer.invoke(ipcChannels.settings.forgetKey)) as IpcResult<null>,
    refreshNow: async () =>
      (await ipcRenderer.invoke(ipcChannels.settings.refreshNow)) as IpcResult<null>,
    onSnapshotUpdated: (handler: SnapshotUpdatedHandler) => {
      const listener = (
        _event: Electron.IpcRendererEvent,
        snapshot: ClaudeUsageSnapshot | null,
      ) => {
        handler(snapshot);
      };
      ipcRenderer.on(ipcChannels.events.snapshotUpdated, listener);
      return () => {
        ipcRenderer.removeListener(ipcChannels.events.snapshotUpdated, listener);
      };
    },
  },
};

contextBridge.exposeInMainWorld('api', api);
