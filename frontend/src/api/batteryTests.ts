// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { BatteryTest, CreateBatteryTestRequest } from '../types/batteryTest';

export function fetchBatteryTests(batteryId: number): Promise<BatteryTest[]> {
  return apiClient.get<BatteryTest[]>(`/batteries/${batteryId}/tests`);
}

export function createBatteryTest(
  batteryId: number,
  data: CreateBatteryTestRequest,
): Promise<BatteryTest> {
  return apiClient.post<BatteryTest>(`/batteries/${batteryId}/tests`, data);
}
