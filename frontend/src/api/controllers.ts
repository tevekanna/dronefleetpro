// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { Controller, CreateControllerRequest } from '../types/controller';

export async function fetchControllers(includeRetired = false): Promise<Controller[]> {
  const q = includeRetired ? '?include_retired=true' : '';
  return apiClient.get(`/controllers${q}`);
}

export async function createController(data: CreateControllerRequest): Promise<Controller> {
  return apiClient.post('/controllers', data);
}

export async function updateController(id: number, data: Partial<CreateControllerRequest>): Promise<Controller> {
  return apiClient.put(`/controllers/${id}`, data);
}

export async function retireController(id: number): Promise<void> {
  return apiClient.delete(`/controllers/${id}`);
}
