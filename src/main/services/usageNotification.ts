import { Notification } from 'electron';
import { decideNearLimitAlerts, decideUsageResets } from '../../common/nearLimitAlerts.ts';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';
import type { SettingsService } from './settings.ts';
import { tryTerminalBell } from './terminalBell.ts';

type LastSeenPercents = { sessionPercent: number; weeklyPercent: number };
type LastSeenPeriodIds = { sessionPeriodId: string; weeklyPeriodId: string };

export class UsageNotificationService {
  private settingsService: SettingsService;
  private lastSeenByOrg = new Map<string, LastSeenPercents>();
  private lastSeenPeriodIdsByOrg = new Map<string, LastSeenPeriodIds>();

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

    let emittedAny = false;

    if (decision.notifySession && decision.sessionPeriodId) {
      this.showNotification(
        '⚠️ Claudometer: Session near limit',
        `5-hour usage is ${Math.round(snapshot.sessionPercent)}% (≥ 90%).`,
      );
      this.settingsService.setSessionNearLimitNotifiedPeriodId(orgId, decision.sessionPeriodId);
      emittedAny = true;
    }

    if (decision.notifyWeekly && decision.weeklyPeriodId) {
      this.showNotification(
        '⚠️ Claudometer: Weekly near limit',
        `Weekly usage is ${Math.round(snapshot.weeklyPercent)}% (≥ 90%).`,
      );
      this.settingsService.setWeeklyNearLimitNotifiedPeriodId(orgId, decision.weeklyPeriodId);
      emittedAny = true;
    }

    this.lastSeenByOrg.set(orgId, {
      sessionPercent: snapshot.sessionPercent,
      weeklyPercent: snapshot.weeklyPercent,
    });

    // Check for usage period resets
    if (this.settingsService.getNotifyOnUsageReset()) {
      const previousPeriods = this.lastSeenPeriodIdsByOrg.get(orgId);

      const resetDecision = decideUsageResets({
        currentSessionResetsAt: snapshot.sessionResetsAt,
        currentWeeklyResetsAt: snapshot.weeklyResetsAt,
        lastSeenSessionPeriodId: previousPeriods?.sessionPeriodId,
        lastSeenWeeklyPeriodId: previousPeriods?.weeklyPeriodId,
        lastNotifiedSessionResetPeriodId:
          this.settingsService.getSessionResetNotifiedPeriodId(orgId) ?? undefined,
        lastNotifiedWeeklyResetPeriodId:
          this.settingsService.getWeeklyResetNotifiedPeriodId(orgId) ?? undefined,
      });

      if (resetDecision.notifySessionReset && resetDecision.sessionResetPeriodId) {
        this.showNotification(
          '🎉 Claudometer: Session period reset!!!!!',
          'Your 5-hour usage window has reset. Happy prompting!',
        );
        this.settingsService.setSessionResetNotifiedPeriodId(orgId, resetDecision.sessionResetPeriodId);
        emittedAny = true;
      }

      if (resetDecision.notifyWeeklyReset && resetDecision.weeklyResetPeriodId) {
        this.showNotification(
          '🎉 Claudometer: Weekly period reset!!!!!',
          'Your weekly usage window has reset. Happy prompting!',
        );
        this.settingsService.setWeeklyResetNotifiedPeriodId(orgId, resetDecision.weeklyResetPeriodId);
        emittedAny = true;
      }
    }

    // Always update the baseline period IDs (even if notifications are disabled)
    // so that enabling notifications later works predictably
    this.lastSeenPeriodIdsByOrg.set(orgId, {
      sessionPeriodId: snapshot.sessionResetsAt ?? '',
      weeklyPeriodId: snapshot.weeklyResetsAt ?? '',
    });

    if (emittedAny && process.platform === 'linux') {
      // Some Linux notification daemons ignore `silent: false`; try a terminal bell as best-effort.
      tryTerminalBell();
    }
  }

  private showNotification(title: string, body: string): void {
    try {
      // `silent: false` means "do not suppress OS sound" (i.e. use system default sound behavior).
      new Notification({ title, body, silent: false }).show();
    } catch {
      // Ignore notification failures (e.g. missing notification daemon on Linux).
    }
  }
}
