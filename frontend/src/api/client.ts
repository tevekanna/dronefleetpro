// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

const BASE_URL = '/api/v1';

// Guard so that several concurrent queries failing with 401 at the same time
// trigger exactly one redirect rather than a burst.
let sessionExpiredHandled = false;

function handleSessionExpired(): void {
  if (sessionExpiredHandled) return;
  sessionExpiredHandled = true;
  localStorage.removeItem('access_token');
  // client.ts lives outside the React Router context, so navigate with a full
  // reload (mirrors handleLogout in App.tsx). This also discards any stale
  // React Query cache belonging to the expired session.
  if (!window.location.pathname.endsWith('/login')) {
    window.location.href = import.meta.env.BASE_URL + 'login';
  }
}

/**
 * Render a 429 response body as English user-facing text.
 *
 * Both rate-limit layers — tower_governor (per-IP) and the per-username
 * Postgres-backed layer — emit the same shape: `{ error, retry_after_seconds }`.
 * We round up to whole minutes so the user gets a concrete "try again
 * later" timeframe instead of the raw seconds.
 */
async function rateLimitedMessage(res: Response): Promise<string> {
  const body = await res.json().catch(() => null);
  const seconds: number | undefined =
    typeof body?.retry_after_seconds === 'number' ? body.retry_after_seconds : undefined;
  if (seconds === undefined) {
    return 'Too many attempts. Please try again in a moment.';
  }
  const minutes = Math.max(1, Math.ceil(seconds / 60));
  return `Too many attempts. Please try again in ${minutes} ${minutes === 1 ? 'minute' : 'minutes'}.`;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const token = localStorage.getItem('access_token');
  const res = await fetch(`${BASE_URL}${path}`, {
    method,
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });

  if (res.status === 429) {
    throw new Error(await rateLimitedMessage(res));
  }

  if (res.status === 401) {
    handleSessionExpired();
    throw new Error('Your session has expired – please sign in again.');
  }

  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? `Request failed: ${res.status}`);
  }

  if (res.status === 204) return undefined as T;
  return res.json();
}

export const apiClient = {
  get:    <T>(path: string) => request<T>('GET', path),
  post:   <T>(path: string, body: unknown) => request<T>('POST', path, body),
  put:    <T>(path: string, body: unknown) => request<T>('PUT', path, body),
  patch:  <T>(path: string, body: unknown) => request<T>('PATCH', path, body),
  delete: <T>(path: string, body?: unknown) => request<T>('DELETE', path, body),
};

export async function login(email: string, password: string): Promise<{ token: string; user: { id: number; name: string; email: string; role: string; user_code: string; org_id: number } }> {
  const res = await fetch(`${BASE_URL}/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (res.status === 429) {
    throw new Error(await rateLimitedMessage(res));
  }
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Login failed' }));
    throw new Error(err.error ?? 'Login failed');
  }
  const data = await res.json();
  localStorage.setItem('access_token', data.token);
  return data;
}

export async function logout(): Promise<void> {
  // Best-effort: tell the backend so it can write a LOGOUT audit row.
  // Failures (network drop, expired token) must never block the user from
  // logging out — clear the local token regardless.
  const token = localStorage.getItem('access_token');
  if (token) {
    try {
      await fetch(`${BASE_URL}/auth/logout`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}` },
      });
    } catch {
      // ignore — local logout still proceeds
    }
  }
  localStorage.removeItem('access_token');
}

export function getToken(): string | null {
  return localStorage.getItem('access_token');
}
