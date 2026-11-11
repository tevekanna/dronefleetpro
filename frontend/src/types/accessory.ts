// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface Accessory {
  id: number;
  org_id: number;
  aircraft_id: number | null;
  name: string;
  category: string | null;
  manufacturer: string | null;
  serial_number: string | null;
  equipment_id: string | null;
  weight_kg: number | null;
  status: 'active' | 'maintenance' | 'retired';
  retirement_notes: string | null;
  created_at: string;
}

export interface CreateAccessoryRequest {
  name: string;
  aircraft_id?: number;
  category?: string;
  manufacturer?: string;
  serial_number?: string;
  equipment_id?: string;
  weight_kg?: number;
}
