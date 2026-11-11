// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface PilotQualification {
  id: number;
  org_id: number;
  pilot_id: number;
  type_name: string;
  issued_date: string | null;
  expiry_date: string | null;
  certificate_ref: string | null;
  notes: string | null;
  created_at: string;
}

export interface Pilot {
  id: number;
  org_id: number;
  name: string;
  email: string | null;
  phone: string | null;
  license_number: string | null;
  license_type: string | null;
  license_expires: string | null;
  is_active: boolean;
  notes: string | null;
  created_at: string;
  qualifications: PilotQualification[];
}

export interface CreatePilotRequest {
  name: string;
  email?: string;
  phone?: string;
  license_number?: string;
  license_type?: string;
  license_expires?: string;
  notes?: string;
  qualifications?: string[];
}

export async function fetchPilots(): Promise<Pilot[]> {
  return apiClient.get('/pilots');
}

export async function createPilot(data: CreatePilotRequest): Promise<Pilot> {
  return apiClient.post('/pilots', data);
}

export async function updatePilot(id: number, data: Partial<CreatePilotRequest & { is_active: boolean }>): Promise<Pilot> {
  return apiClient.put(`/pilots/${id}`, data);
}

export async function deactivatePilot(id: number): Promise<void> {
  return apiClient.delete(`/pilots/${id}`);
}
