// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { Accessory, CreateAccessoryRequest } from '../types/accessory';

export async function fetchAccessories(includeRetired = false): Promise<Accessory[]> {
  const q = includeRetired ? '?include_retired=true' : '';
  return apiClient.get(`/accessories${q}`);
}

export async function createAccessory(data: CreateAccessoryRequest): Promise<Accessory> {
  return apiClient.post('/accessories', data);
}

export async function updateAccessory(id: number, data: Partial<CreateAccessoryRequest> & Record<string, unknown>): Promise<Accessory> {
  return apiClient.put(`/accessories/${id}`, data);
}

export async function retireAccessory(id: number): Promise<void> {
  return apiClient.delete(`/accessories/${id}`);
}
