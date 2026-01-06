/**
 * Fetch with automatic timeout using AbortController
 * @param input - URL or Request object
 * @param init - Fetch options
 * @param timeoutMs - Timeout in milliseconds (default: 30000ms = 30s)
 * @returns Promise<Response>
 * @throws DOMException with name 'AbortError' on timeout
 */
export async function fetchWithTimeout(
  input: RequestInfo | URL,
  init?: RequestInit,
  timeoutMs = 30000,
): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(input, {
      ...init,
      signal: controller.signal,
    });
    return response;
  } finally {
    clearTimeout(timeoutId);
  }
}
