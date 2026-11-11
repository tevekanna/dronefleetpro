// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface Battery {
  id: number;
  org_id: number;
  aircraft_id: number | null;
  serial_number: string;
  equipment_id: string | null;
  cycle_count: number;
  max_cycles: number;
  capacity_wh: number | null;
  weight_kg: number | null;
  compatible_model: string | null;
  compatible_model_id: number | null;
  last_used: string | null;
  status: 'active' | 'maintenance' | 'retired';
  retirement_notes: string | null;
  created_at: string;
}

export interface CreateBatteryRequest {
  serial_number: string;
  equipment_id?: string;
  aircraft_id?: number;
  capacity_wh?: number;
  weight_kg?: number;
  max_cycles?: number;
  compatible_model?: string;
  compatible_model_id?: number;
}
