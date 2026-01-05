export type NearLimitPeriod = 'session' | 'weekly';

export type NearLimitAlertDecision = {
  notifySession: boolean;
  notifyWeekly: boolean;
  sessionPeriodId: string | null;
  weeklyPeriodId: string | null;
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
