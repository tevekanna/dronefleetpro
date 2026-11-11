// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { test as base, expect, type Page, type APIRequestContext } from '@playwright/test';
import { clearLoginRateLimit } from './test-data';

// Direct backend URL for API-level setup. The Vite dev proxy targets the
// docker network hostname `backend:3000` which only resolves inside the
// frontend container; Playwright runs on the host, so we hit the exposed
// port directly.
export const BACKEND_URL = process.env.E2E_BACKEND_URL ?? 'http://localhost:3000';
export const API_BASE = `${BACKEND_URL}/api/v1`;

export const ADMIN_EMAIL = process.env.E2E_ADMIN_EMAIL ?? 'admin@dronefleetpro.local';
export const ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD ?? 'dronefleet2025';

interface LoginResponse {
  token: string;
  user: { id: number; name: string; email: string; role: string; user_code: string; org_id: number };
}

/**
 * Log the admin user in via the backend API and return the JWT.
 *
 * The frontend stores the JWT in localStorage as `access_token` and reads it
 * back on each fetch. Tests must seed the same key before navigating to a
 * protected route, otherwise the React app will redirect to /login.
 */
export async function loginAsAdmin(request: APIRequestContext): Promise<string> {
  const res = await request.post(`${API_BASE}/auth/login`, {
    data: { email: ADMIN_EMAIL, password: ADMIN_PASSWORD },
  });
  if (!res.ok()) {
    throw new Error(
      `Admin login failed: ${res.status()} ${await res.text()}. ` +
        `Set E2E_ADMIN_EMAIL / E2E_ADMIN_PASSWORD if the default admin password has been rotated.`,
    );
  }
  const body = (await res.json()) as LoginResponse;
  return body.token;
}

type WorkerFixtures = {
  adminToken: string;
};

type TestFixtures = {
  adminPage: Page;
};

export const test = base.extend<TestFixtures, WorkerFixtures>({
  // Worker-scoped: log in once per worker process. Avoids exhausting the
  // backend's per-username rate limit (10 attempts / 15 min, hardcoded —
  // see backend/src/services/rate_limit.rs) when many tests run in sequence.
  adminToken: [
    async ({ playwright }, use) => {
      // Clear the rate-limit window before logging in. Iterating on E2E
      // tests during development burns through the 10-attempt budget fast,
      // and the backend exposes no way to set it. The fixture is dev-only
      // and the cleared rows are audit-irrelevant for test credentials.
      try {
        clearLoginRateLimit(ADMIN_EMAIL);
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn('clearLoginRateLimit failed (continuing):', err);
      }
      // Each worker gets its own request context — `request` test-scope
      // fixture isn't visible here.
      const request = await playwright.request.newContext();
      try {
        const token = await loginAsAdmin(request);
        await use(token);
      } finally {
        await request.dispose();
      }
    },
    { scope: 'worker' },
  ],

  adminPage: async ({ browser, adminToken }, use) => {
    const context = await browser.newContext();
    // Seed localStorage on the origin before the React app boots. addInitScript
    // runs on every navigation, so the token survives reloads within the test.
    await context.addInitScript((token) => {
      window.localStorage.setItem('access_token', token);
    }, adminToken);
    const page = await context.newPage();
    await use(page);
    await context.close();
  },
});

export { expect };
