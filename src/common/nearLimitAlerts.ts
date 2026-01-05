export type NearLimitPeriod = 'session' | 'weekly';

export type NearLimitAlertDecision = {
  notifySession: boolean;
  notifyWeekly: boolean;
  sessionPeriodId: string | null;
  weeklyPeriodId: string | null;
};

export type UsageResetDecision = {
  notifySessionReset: boolean;
  notifyWeeklyReset: boolean;
  sessionResetPeriodId: string | null;
  weeklyResetPeriodId: string | null;
};

const NEAR_LIMIT_THRESHOLD_PERCENT = 90;
const UNKNOWN_PERIOD_ID = 'unknown';

function normalizePeriodId(resetsAt: string | undefined): string {
  const trimmed = resetsAt?.trim();
  return trimmed ? trimmed : UNKNOWN_PERIOD_ID;
}

function shouldNotify(params: {
  currentPercent: number;
  previousPercent: number | null;
  currentPeriodId: string;
  lastNotifiedPeriodId: string | null;
}): boolean {
  if (params.currentPercent < NEAR_LIMIT_THRESHOLD_PERCENT) return false;
  if (params.lastNotifiedPeriodId === params.currentPeriodId) return false;
  if (params.previousPercent === null) return true;
  return params.previousPercent < NEAR_LIMIT_THRESHOLD_PERCENT;
}

export function decideNearLimitAlerts(params: {
  currentSessionPercent: number;
  currentWeeklyPercent: number;
  currentSessionResetsAt?: string;
  currentWeeklyResetsAt?: string;
  previousSessionPercent?: number;
  previousWeeklyPercent?: number;
  lastNotifiedSessionPeriodId?: string;
  lastNotifiedWeeklyPeriodId?: string;
}): NearLimitAlertDecision {
  const sessionPeriodId = normalizePeriodId(params.currentSessionResetsAt);
  const weeklyPeriodId = normalizePeriodId(params.currentWeeklyResetsAt);

  const notifySession = shouldNotify({
    currentPercent: params.currentSessionPercent,
    previousPercent:
      typeof params.previousSessionPercent === 'number' ? params.previousSessionPercent : null,
    currentPeriodId: sessionPeriodId,
    lastNotifiedPeriodId: params.lastNotifiedSessionPeriodId?.trim() || null,
  });

  const notifyWeekly = shouldNotify({
    currentPercent: params.currentWeeklyPercent,
    previousPercent:
      typeof params.previousWeeklyPercent === 'number' ? params.previousWeeklyPercent : null,
    currentPeriodId: weeklyPeriodId,
    lastNotifiedPeriodId: params.lastNotifiedWeeklyPeriodId?.trim() || null,
  });

  return {
    notifySession,
    notifyWeekly,
    sessionPeriodId: notifySession ? sessionPeriodId : null,
    weeklyPeriodId: notifyWeekly ? weeklyPeriodId : null,
  };
}

/**
 * Determine whether to notify about a usage period reset.
 * A reset is detected when the current period ID differs from the last seen period ID.
 * Does NOT notify on first observation (no baseline).
 */
function shouldNotifyReset(params: {
  currentPeriodId: string;
  lastSeenPeriodId: string | null;
  lastNotifiedResetPeriodId: string | null;
}): boolean {
  // No current period ID - cannot detect reset
  if (!params.currentPeriodId || params.currentPeriodId === UNKNOWN_PERIOD_ID) {
    return false;
  }

  // No baseline yet - don't notify on first observation
  if (params.lastSeenPeriodId === null) {
    return false;
  }

  // Period hasn't changed - no reset
  if (params.currentPeriodId === params.lastSeenPeriodId) {
    return false;
  }

  // Already notified for this new period - don't notify again
  if (params.lastNotifiedResetPeriodId === params.currentPeriodId) {
    return false;
  }

  // Period changed and we haven't notified for the new period yet
  return true;
}

export function decideUsageResets(params: {
  currentSessionResetsAt?: string;
  currentWeeklyResetsAt?: string;
  lastSeenSessionPeriodId?: string;
  lastSeenWeeklyPeriodId?: string;
  lastNotifiedSessionResetPeriodId?: string;
  lastNotifiedWeeklyResetPeriodId?: string;
}): UsageResetDecision {
  const sessionPeriodId = normalizePeriodId(params.currentSessionResetsAt);
  const weeklyPeriodId = normalizePeriodId(params.currentWeeklyResetsAt);

  const notifySessionReset = shouldNotifyReset({
    currentPeriodId: sessionPeriodId,
    lastSeenPeriodId: params.lastSeenSessionPeriodId?.trim() || null,
    lastNotifiedResetPeriodId: params.lastNotifiedSessionResetPeriodId?.trim() || null,
  });

  const notifyWeeklyReset = shouldNotifyReset({
    currentPeriodId: weeklyPeriodId,
    lastSeenPeriodId: params.lastSeenWeeklyPeriodId?.trim() || null,
    lastNotifiedResetPeriodId: params.lastNotifiedWeeklyResetPeriodId?.trim() || null,
  });

  return {
    notifySessionReset,
    notifyWeeklyReset,
    sessionResetPeriodId: notifySessionReset ? sessionPeriodId : null,
    weeklyResetPeriodId: notifyWeeklyReset ? weeklyPeriodId : null,
  };
}
