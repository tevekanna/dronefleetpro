// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export async function fetchSettings(): Promise<Record<string, string>> {
  return apiClient.get('/settings');
}

export async function saveSetting(key: string, value: string): Promise<void> {
  await apiClient.put(`/settings/${key}`, { value });
}
