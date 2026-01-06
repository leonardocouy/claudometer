/**
 * Sanitization utilities to prevent sensitive data from appearing in logs
 */

/**
 * Sanitize a string by redacting sensitive tokens and credentials
 * Supports:
 * - sessionKey (Claude web session cookies)
 * - sk-ant-sid01-* (Claude web session tokens)
 * - sk-ant-api03-* (OAuth API tokens)
 * - Authorization: Bearer headers
 */
export function sanitizeString(input: string): string {
  let sanitized = input;

  // Redact sessionKey cookie values
  sanitized = sanitized.replaceAll(/sessionKey=[^;\s]+/gi, 'sessionKey=REDACTED');

  // Redact Claude web session tokens (sk-ant-sid01-*)
  sanitized = sanitized.replaceAll(/sk-ant-sid01-[A-Za-z0-9_-]+/g, 'sk-ant-sid01-REDACTED');

  // Redact OAuth API tokens (sk-ant-api03-*)
  sanitized = sanitized.replaceAll(/sk-ant-api03-[A-Za-z0-9_-]+/g, 'sk-ant-api03-REDACTED');

  // Redact Authorization Bearer tokens
  sanitized = sanitized.replaceAll(/Authorization:\s*Bearer\s+[A-Za-z0-9._-]+/gi, 'Authorization: Bearer REDACTED');

  return sanitized;
}

/**
 * Sanitize error messages before logging
 * Handles both string and Error objects
 */
export function sanitizeError(error: unknown): string {
  if (error instanceof Error) {
    return sanitizeString(`${error.name}: ${error.message}`);
  }
  if (typeof error === 'string') {
    return sanitizeString(error);
  }
  if (typeof error === 'object' && error !== null) {
    try {
      return sanitizeString(JSON.stringify(error));
    } catch {
      return '[Unable to stringify error object]';
    }
  }
  return sanitizeString(String(error));
}

/**
 * Sanitize an object by recursively sanitizing all string values
 * Useful for sanitizing entire context objects before logging
 */
export function sanitizeObject<T>(obj: T): T {
  if (typeof obj === 'string') {
    return sanitizeString(obj) as T;
  }
  if (obj === null || obj === undefined) {
    return obj;
  }
  if (Array.isArray(obj)) {
    return obj.map(sanitizeObject) as T;
  }
  if (typeof obj === 'object') {
    const sanitized: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      sanitized[key] = sanitizeObject(value);
    }
    return sanitized as T;
  }
  return obj;
}
