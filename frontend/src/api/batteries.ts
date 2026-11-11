// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { Battery, CreateBatteryRequest } from '../types/battery';

export async function fetchBatteries(includeRetired = false): Promise<Battery[]> {
  const q = includeRetired ? '?include_retired=true' : '';
  return apiClient.get(`/batteries${q}`);
}

export async function createBattery(data: CreateBatteryRequest): Promise<Battery> {
  return apiClient.post('/batteries', data);
}

export async function updateBattery(id: number, data: Partial<CreateBatteryRequest>): Promise<Battery> {
  return apiClient.put(`/batteries/${id}`, data);
}

export async function retireBattery(id: number): Promise<void> {
  return apiClient.delete(`/batteries/${id}`);
}
