// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface Maintenance {
  id: number;
  org_id: number;
  aircraft_id: number | null;
  performed_date: string;
  created_by: string;
  remark: string | null;
  action_taken: string | null;
  maint_category: string | null;
  maint_type: string | null;
  sign_off: string;
  created_at: string;
}
