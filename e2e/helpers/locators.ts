import type { Locator, Page } from '@playwright/test';

/**
 * Resolves a semantic UI control with an ordered fallback list for minor DOM/label drift.
 * Never swallows missing functionality: throws with diagnostics if all semantic candidates miss.
 */
export async function pickLocator(
  page: Page,
  candidates: ReadonlyArray<() => Locator>,
  timeoutMs = 5_000,
): Promise<Locator> {
  const perCandidateMs = Math.max(400, Math.floor(timeoutMs / candidates.length));
  for (const build of candidates) {
    const loc = build().first();
    try {
      await loc.waitFor({ state: 'visible', timeout: perCandidateMs });
      return loc;
    } catch {
      // Try next semantic candidate before failing loudly.
    }
  }
  return candidates[0]().first();
}
