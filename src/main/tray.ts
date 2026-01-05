/**
 * Tray Manager - system tray icon and menu
 */

import { Menu, nativeImage, Tray } from 'electron';
import type { ClaudeUsageSnapshot } from '../common/types.ts';

export interface TrayServiceOptions {
  onOpenSettings: () => void;
  onRefreshNow: () => void;
  onQuit: () => void;
}

export class TrayService {
  private tray: Tray | null = null;
  private latestSnapshot: ClaudeUsageSnapshot | null = null;
  private options: TrayServiceOptions;

  constructor(options: TrayServiceOptions) {
    this.options = options;
    this.createTray();
  }

  private createTray(): void {
    const icon = this.createIcon();
    this.tray = new Tray(icon);
    this.tray.setToolTip('Claudometer');
    this.updateMenu();

    this.tray.on('click', () => {
      this.tray?.popUpContextMenu();
    });
  }

  private createIcon(): Electron.NativeImage {
    const size = 16;
    const canvas = Buffer.alloc(size * size * 4);

    const status = this.latestSnapshot?.status ?? 'missing_key';
    let r = 60,
      g = 60,
      b = 60;

    if (status === 'ok') {
      r = 30;
      g = 160;
      b = 60;
    } else if (status === 'unauthorized') {
      r = 220;
      g = 40;
      b = 40;
    } else if (status === 'rate_limited') {
      r = 220;
      g = 140;
      b = 40;
    }

    const center = size / 2;
    const radius = size / 2 - 1;

    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        const dx = x - center;
        const dy = y - center;
        const distance = Math.sqrt(dx * dx + dy * dy);
        const offset = (y * size + x) * 4;

        if (distance <= radius) {
          canvas[offset] = r;
          canvas[offset + 1] = g;
          canvas[offset + 2] = b;
          canvas[offset + 3] = 255;
        } else {
          canvas[offset + 3] = 0;
        }
      }
    }

    const image = nativeImage.createFromBuffer(canvas, { width: size, height: size });
    image.setTemplateImage?.(false);
    return image;
  }

  updateSnapshot(snapshot: ClaudeUsageSnapshot | null): void {
    this.latestSnapshot = snapshot;
    if (!this.tray) return;
    this.tray.setImage(this.createIcon());
    this.updateMenu();
  }

  private formatPercent(value: number | undefined): string {
    if (typeof value !== 'number') return '--%';
    return `${Math.round(value)}%`;
  }

  private formatTime(iso: string | undefined): string {
    if (!iso) return '';
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return '';
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  private updateMenu(): void {
    if (!this.tray) return;
    const snapshot = this.latestSnapshot;

    const header =
      snapshot?.status === 'ok'
        ? 'Claude Usage'
        : snapshot?.status === 'missing_key'
          ? 'Claude Usage (needs session key)'
          : snapshot?.status === 'unauthorized'
            ? 'Claude Usage (unauthorized)'
            : snapshot?.status === 'rate_limited'
              ? 'Claude Usage (rate limited)'
              : 'Claude Usage (error)';

    const items: Electron.MenuItemConstructorOptions[] = [
      { label: header, enabled: false },
      { type: 'separator' },
    ];

    if (snapshot?.status === 'ok') {
      items.push(
        {
          label: `Session: ${this.formatPercent(snapshot.sessionPercent)} ${
            snapshot.sessionResetsAt ? `(resets ${this.formatTime(snapshot.sessionResetsAt)})` : ''
          }`,
          enabled: false,
        },
        {
          label: `Weekly: ${this.formatPercent(snapshot.weeklyPercent)} ${
            snapshot.weeklyResetsAt ? `(resets ${this.formatTime(snapshot.weeklyResetsAt)})` : ''
          }`,
          enabled: false,
        },
        {
          label: `${snapshot.modelWeeklyName ?? 'Model'} (weekly): ${this.formatPercent(
            snapshot.modelWeeklyPercent,
          )} ${
            snapshot.modelWeeklyResetsAt
              ? `(resets ${this.formatTime(snapshot.modelWeeklyResetsAt)})`
              : ''
          }`,
          enabled: false,
        },
      );
    } else if (snapshot?.errorMessage) {
      items.push({ label: snapshot.errorMessage, enabled: false });
    }

    const lastUpdatedAt = snapshot?.lastUpdatedAt;
    if (lastUpdatedAt) {
      const updated = new Date(lastUpdatedAt);
      const text = Number.isNaN(updated.getTime()) ? lastUpdatedAt : updated.toLocaleString();
      items.push({ type: 'separator' }, { label: `Last updated: ${text}`, enabled: false });
    }

    items.push(
      { type: 'separator' },
      { label: 'Refresh now', click: () => this.options.onRefreshNow() },
      { label: 'Open Settings…', click: () => this.options.onOpenSettings() },
      { type: 'separator' },
      { label: 'Quit', click: () => this.options.onQuit() },
    );

    const menu = Menu.buildFromTemplate(items);
    this.tray.setContextMenu(menu);
  }

  destroy(): void {
    this.tray?.destroy();
    this.tray = null;
  }
}
