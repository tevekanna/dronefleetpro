// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

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
}
