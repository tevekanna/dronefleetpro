// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { test, expect, ADMIN_EMAIL, ADMIN_PASSWORD } from './fixtures/auth';
import { clearLoginRateLimit } from './fixtures/test-data';

/**
 * Real UI login coverage. Unlike every other spec (which seeds the JWT into
 * localStorage via the adminPage fixture and skips the login screen), this one
 * uses the PLAIN, unauthenticated `page` fixture and drives the actual login
 * form end-to-end: form fill -> submit -> backend auth -> redirect to the
 * authenticated AppShell.
 *
 * Prerequisite: docker-compose dev stack must be running. See e2e/README.md.
 */
test.describe('Login form', () => {
  test.beforeAll(() => {
    // Clear the backend's 10-attempt / 15-min rolling rate-limit window so
    // repeated dev iterations don't start returning 401s. Best-effort, same
    // pattern as the auth fixture.
    try {
      clearLoginRateLimit(ADMIN_EMAIL);
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('clearLoginRateLimit failed (continuing):', err);
    }
  });

  test('admin can sign in through the login form', async ({ page }) => {
    await page.goto('/login');

    await page.locator('input[type="email"]').fill(ADMIN_EMAIL);
    await page.locator('input[type="password"]').fill(ADMIN_PASSWORD);
    await page.getByRole('button', { name: /sign in/i }).click();

    // Successful login redirects to the dashboard and renders the AppShell,
    // whose sidebar footer holds the Sign Out button (only present once
    // authenticated).
    await expect(page).toHaveURL(/\/dashboard/);
    await expect(page.getByRole('button', { name: /sign out/i })).toBeVisible();
  });

  test('wrong password shows an error', async ({ page }) => {
    // Clear the window again so this second attempt isn't counted against the
    // budget consumed by the successful login above.
    try {
      clearLoginRateLimit(ADMIN_EMAIL);
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn('clearLoginRateLimit failed (continuing):', err);
    }

    await page.goto('/login');

    await page.locator('input[type="email"]').fill(ADMIN_EMAIL);
    await page.locator('input[type="password"]').fill('definitely-not-the-password');
    await page.getByRole('button', { name: /sign in/i }).click();

    // LoginPage renders the thrown error text in a <p class=error>. We stay on
    // the login page and surface the failure rather than redirecting.
    await expect(page.locator('p').filter({ hasText: /invalid|incorrect|failed|credentials/i })).toBeVisible();
    await expect(page).toHaveURL(/\/login/);
  });
});
