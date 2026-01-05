import { Notification, shell } from 'electron';
import { createRequire } from 'node:module';
import { decideNearLimitAlerts } from '../../common/nearLimitAlerts.ts';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';
import type { SettingsService } from './settings.ts';

type LastSeenPercents = { sessionPercent: number; weeklyPercent: number };

const require = createRequire(import.meta.url);

function tryNotifyWithNodeNotifier(title: string, body: string): boolean {
  try {
    const mod = require('node-notifier') as unknown;
    const notifier = (mod as { default?: unknown })?.default ?? mod;
    if (!notifier || typeof notifier !== 'object') return false;
    const notify = (notifier as { notify?: unknown }).notify;
    if (typeof notify !== 'function') return false;

    (notify as (options: Record<string, unknown>) => void)({
      title,
      message: body,
      sound: true,
      wait: false,
      appName: 'Claudometer',
    });

    return true;
  } catch {
    return false;
  }
}

export class UsageNotificationService {
  private settingsService: SettingsService;
  private lastSeenByOrg = new Map<string, LastSeenPercents>();

  constructor(settingsService: SettingsService) {
    this.settingsService = settingsService;
  }

  maybeNotify(snapshot: ClaudeUsageSnapshot | null): void {
    if (!snapshot || snapshot.status !== 'ok') return;
    if (!Notification.isSupported()) return;

    const orgId = snapshot.organizationId;
    const previous = this.lastSeenByOrg.get(orgId);

    const decision = decideNearLimitAlerts({
      currentSessionPercent: snapshot.sessionPercent,
      currentWeeklyPercent: snapshot.weeklyPercent,
      currentSessionResetsAt: snapshot.sessionResetsAt,
      currentWeeklyResetsAt: snapshot.weeklyResetsAt,
      previousSessionPercent: previous?.sessionPercent,
      previousWeeklyPercent: previous?.weeklyPercent,
      lastNotifiedSessionPeriodId:
        this.settingsService.getSessionNearLimitNotifiedPeriodId(orgId) ?? undefined,
      lastNotifiedWeeklyPeriodId:
        this.settingsService.getWeeklyNearLimitNotifiedPeriodId(orgId) ?? undefined,
    });

    if (decision.notifySession && decision.sessionPeriodId) {
      this.showNotification(
        'Claudometer: Session near limit',
        `5-hour usage is ${Math.round(snapshot.sessionPercent)}% (≥ 90%).`,
      );
      this.settingsService.setSessionNearLimitNotifiedPeriodId(orgId, decision.sessionPeriodId);
    }

    if (decision.notifyWeekly && decision.weeklyPeriodId) {
      this.showNotification(
        'Claudometer: Weekly near limit',
        `Weekly usage is ${Math.round(snapshot.weeklyPercent)}% (≥ 90%).`,
      );
      this.settingsService.setWeeklyNearLimitNotifiedPeriodId(orgId, decision.weeklyPeriodId);
    }

    this.lastSeenByOrg.set(orgId, {
      sessionPercent: snapshot.sessionPercent,
      weeklyPercent: snapshot.weeklyPercent,
    });
  }

  private showNotification(title: string, body: string): void {
    try {
      if (tryNotifyWithNodeNotifier(title, body)) return;

      shell.beep();
      new Notification({ title, body, silent: false }).show();
    } catch {
      // Ignore notification failures (e.g. missing notification daemon on Linux).
    }
  }
}
