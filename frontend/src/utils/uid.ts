// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

/**
 * Returns a unique-enough id for local React keys and draft rows.
 *
 * crypto.randomUUID() only exists in secure contexts (HTTPS or localhost),
 * so over plain HTTP on a LAN IP it is undefined and throws. These ids are
 * not security-sensitive, so a timestamp + random fallback is fine.
 */
export function uid(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
