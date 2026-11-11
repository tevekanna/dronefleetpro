// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface Controller {
  id: number;
  org_id: number;
  serial_number: string;
  equipment_id: string | null;
  model: string | null;
  nickname: string | null;
  aircraft_id: number | null;
  status: 'active' | 'maintenance' | 'retired';
  notes: string | null;
  compatible_models: string | null;
  retirement_notes: string | null;
  created_at: string;
}

export interface CreateControllerRequest {
  serial_number: string;
  equipment_id?: string;
  model?: string;
  nickname?: string;
  aircraft_id?: number;
  notes?: string;
  compatible_models?: string;
}
