import { expect, test } from '@playwright/test';
import { setupAdaptiveCoachingStubs } from './helpers/apiStubs';
import { pickLocator } from './helpers/locators';

test.describe('FluentDesk AI adaptive coaching & FreeLLMAPI E2E flows', () => {
  test('drives Gentle default, structured coaching, clean suppression, review, mode switch, and FreeLLMAPI UI', async ({ page }) => {
    const pageErrors: string[] = [];
    page.on('pageerror', (err) => pageErrors.push(err.message));
    page.on('console', (msg) => {
      if (msg.type() === 'error') pageErrors.push(msg.text());
    });

    const recorded = await setupAdaptiveCoachingStubs(page);
    await page.goto('/');
    expect(pageErrors).toEqual([]);

    const modeSelect = await pickLocator(page, [
      () => page.getByLabel('Feedback Intensity'),
      () => page.locator('#correction-mode-select'),
    ], 15_000);
    await expect(modeSelect).toHaveValue('Gentle');

    // Open Models & Keys drawer to confirm FreeLLMAPI (Local Gateway) is exposed cleanly.
    const keysToggle = await pickLocator(page, [
      () => page.getByRole('button', { name: 'Models & Keys' }),
      () => page.getByText('Models & Keys'),
    ]);
    await keysToggle.click();
    await expect(page.getByRole('textbox', { name: 'FreeLLMAPI (Local Gateway) model' })).toHaveValue('auto:fast');

    // Flow 1: Start scenario -> enters Listening with opener.
    const startScenarioBtn = await pickLocator(page, [
      () => page.getByRole('button', { name: 'Practice This Scenario' }),
      () => page.getByRole('button', { name: 'Start Speaking' }),
    ]);
    await startScenarioBtn.click();
    const replyCard = page.getByRole('region', { name: 'AI Tutor Response' });
    await expect(replyCard.getByText('Welcome to Sunrise Cafe! What can I get for you?')).toBeVisible();

    const input = await pickLocator(page, [
      () => page.getByPlaceholder(/Speak into mic|type your English/i),
      () => page.locator('.voice-input-row input[type="text"]'),
    ]);
    const sendBtn = await pickLocator(page, [
      () => page.getByRole('button', { name: 'Send', exact: true }),
      () => page.locator('.voice-input-row button[type="submit"]'),
    ]);

    // Flow 2: Structured coaching ("Try saying" + Listen + why) without auto-speaking feedback.
    await input.fill('Give me medium latte');
    await sendBtn.click();

    const coachingBox = replyCard.getByRole('complementary', { name: 'Coaching feedback' });
    await expect(coachingBox).toBeVisible();
    await expect(coachingBox).toContainText('Try saying');
    await expect(coachingBox).toContainText('Could I get a medium latte, please?');
    await expect(coachingBox).toContainText("At a cafe counter, 'Could I get...' sounds warmer");

    await expect.poll(() => page.evaluate(() => (window as unknown as { __spokenTexts: string[] }).__spokenTexts.length)).toBeGreaterThanOrEqual(2);
    const spokenBeforeListen = await page.evaluate(() => (window as unknown as { __spokenTexts: string[] }).__spokenTexts);
    expect(spokenBeforeListen.some((s) => s.includes('One medium latte coming right up!'))).toBe(true);
    expect(spokenBeforeListen.some((s) => s.includes('Could I get a medium latte'))).toBe(false);

    // Clicking "Listen" speaks properWay on demand; Transcript stays dialogue-only.
    await coachingBox.getByRole('button', { name: 'Listen' }).click();
    await expect.poll(() => page.evaluate(() => (window as unknown as { __spokenTexts: string[] }).__spokenTexts.at(-1))).toBe(
      'Could I get a medium latte, please?',
    );
    await expect(page.getByRole('region', { name: 'Conversation Transcript' })).not.toContainText('Try saying');

    // Flow 3: Clean turn hides coaching box completely.
    await input.fill('Iced sounds great, thank you!');
    await sendBtn.click();
    await expect(replyCard.getByText('Got it, iced works great! Anything else today?')).toBeVisible();
    await expect(coachingBox).toHaveCount(0);

    // Flow 5: Stop session -> SessionReviewCard shows "<properWay> — <why>" under Grammar focuses & re-enables mode selector.
    const stopBtn = await pickLocator(page, [
      () => page.getByRole('button', { name: 'Stop Session' }),
      () => page.locator('[data-stop-btn="true"]'),
    ]);
    await stopBtn.click();
    const review = page.getByLabel('Session Review');
    await expect(review).toBeVisible();
    await expect(review).toContainText(
      "Could I get a medium latte, please? — At a cafe counter, 'Could I get...' sounds warmer than 'Give me...'.",
    );

    // Flow 4: Switch CorrectionMode to Detailed -> start session 2 -> legacy "Coaching Tip" renders & sends Detailed mode.
    await expect(modeSelect).toBeEnabled();
    await modeSelect.selectOption('Detailed');
    await expect.poll(() => recorded.preferencePatches.at(-1)?.correctionMode).toBe('Detailed');
    await page.getByRole('button', { name: 'Start Speaking' }).click();
    await input.fill('legacy phrasing test');
    await sendBtn.click();
    await expect(coachingBox).toContainText('Coaching Tip');
    await expect(coachingBox).toContainText('Try past tense: "I went".');
    expect(recorded.turnModes.at(-1)).toBe('Detailed');
  });
});
