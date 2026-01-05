import { ipcMain } from 'electron';
import { ipcChannels } from '../../common/ipc.ts';
import type { AppController } from '../app-controller.ts';

export function registerIpcHandlers(controller: AppController): void {
  ipcMain.handle(ipcChannels.settings.getState, async () => controller.getState());
  ipcMain.handle(ipcChannels.settings.save, async (_event, payload) =>
    controller.saveSettings(payload),
  );
  ipcMain.handle(ipcChannels.settings.forgetKey, async () => controller.forgetKey());
  ipcMain.handle(ipcChannels.settings.refreshNow, async () => controller.refreshNow());
}
