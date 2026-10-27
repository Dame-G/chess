/** The longest the event stream waits between two attempts. */
export const RETRY_CAP_MS = 10_000;

/**
 * How long to wait before the next attempt to reach the server: `base` doubled for each failure in a row, held at
 * `cap`, then spread over its upper half so many tabs that lost the server together do not come back together.
 * `attempt` is 0 for the first failure after a success; `random` is a number in [0, 1).
 */
export function retryDelay(attempt: number, random = Math.random(), base = 1000, cap = RETRY_CAP_MS): number {
  const ceiling = Math.min(cap, base * 2 ** Math.min(Math.max(0, attempt), 30));
  return Math.round(ceiling / 2 + (ceiling / 2) * random);
}
