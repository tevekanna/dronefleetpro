// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface HubFleetSummary {
  active_aircraft:    number;
  overdue_schedules:  number;
  due_soon_schedules: number;
  open_defects:       number;
}

export interface HubAlert {
  schedule_id:    number;
  schedule_name:  string;
  interval_type:  string;
  equipment_id:   string | null;
  aircraft_name:  string | null;
  severity:       'overdue' | 'due_soon';
  overdue_days:   number | null;
  overdue_hours:  number | null;
  overdue_cycles: number | null;
  due_in_days:    number | null;
}

export interface HubUpcoming {
  schedule_id:   number;
  schedule_name: string;
  next_due_at:   string | null;
  equipment_id:  string | null;
  aircraft_name: string | null;
}

export interface HubRecentActivity {
  id:             number;
  performed_date: string;
  maint_type:     string | null;
  created_by:     string;
  equipment_id:   string | null;
  equipment_name: string | null;
}

export interface HubOpenDefect {
  id:           number;
  defect_date:  string;
  description:  string;
  status:       string;
  equipment_id: string | null;
}

export interface MaintenanceHubData {
  summary:         HubFleetSummary;
  alerts:          HubAlert[];
  upcoming:        HubUpcoming[];
  recent_activity: HubRecentActivity[];
  open_defects:    HubOpenDefect[];
}

export function fetchMaintenanceHub(): Promise<MaintenanceHubData> {
  return apiClient.get<MaintenanceHubData>('/maintenance/hub');
}
