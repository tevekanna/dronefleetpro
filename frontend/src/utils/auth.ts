// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

/**
 * Decodes the primary role from the stored JWT access token.
 * Returns null when no token is present or decoding fails.
 *
 * NOTE: this is for DISPLAY only. Authorisation must never branch on the
 * single primary role — use `hasAnyRole` / `getRolesFromToken` so a user
 * with several roles (e.g. viewer + pilot) is judged on the full set.
 */
export function getRoleFromToken(): string | null {
  const token = localStorage.getItem('access_token');
  if (!token) return null;
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    return (payload.role as string) ?? null;
  } catch {
    return null;
  }
}

/**
 * Decodes the full RBAC role set from the stored JWT access token.
 *
 * Prefers the `roles` array (multi-role tokens). Falls back to the legacy
 * single `role` claim so tokens minted before the multi-role rollout keep
 * working until they expire. Returns [] when no token is present, decoding
 * fails, or neither claim exists.
 */
export function getRolesFromToken(): string[] {
  const token = localStorage.getItem('access_token');
  if (!token) return [];
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    if (Array.isArray(payload.roles)) {
      return payload.roles as string[];
    }
    if (typeof payload.role === 'string') {
      return [payload.role];
    }
    return [];
  } catch {
    return [];
  }
}

/** True when the current token's role set intersects any of `roles`. */
export function hasAnyRole(...roles: string[]): boolean {
  const held = getRolesFromToken();
  return held.some((r) => roles.includes(r));
}

/**
 * True when the given user's role set intersects any of `roles`.
 *
 * For useCurrentUser / users-list consumers. Prefers `user.roles`; falls
 * back to the denormalised primary `user.role` when the array is absent.
 * Returns false for a null/undefined user.
 */
export function userHasAnyRole(
  user: { role?: string; roles?: string[] } | null | undefined,
  ...roles: string[]
): boolean {
  if (!user) return false;
  const held =
    Array.isArray(user.roles) && user.roles.length > 0
      ? user.roles
      : user.role
        ? [user.role]
        : [];
  return held.some((r) => roles.includes(r));
}

/** Returns true when the current user is an admin or technician. */
export function canEdit(): boolean {
  return hasAnyRole('admin', 'technician');
}

/** Returns true when the current user can report defects (all roles except viewer-only). */
export function canReportDefect(): boolean {
  return hasAnyRole('admin', 'safety_officer', 'technician', 'pilot');
}
