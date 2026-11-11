// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type BatteryTestResult = 'pass' | 'warning' | 'fail';
export type SwellingStatus = 'none' | 'minor' | 'severe';

/**
 * A battery health/capacity test record. Mirrors the backend `battery_tests`
 * row. `capacity_measured` is in Wh and `internal_resistance` is in mΩ by
 * convention (legacy column names reused for this feature).
 */
export interface BatteryTest {
  id: number;
  org_id: number;
  battery_id: number;
  test_date: string;
  test_type: string;
  result: BatteryTestResult;
  capacity_measured: number | null;
  internal_resistance: number | null;
  swelling_status: SwellingStatus;
  cell_voltages: number[] | null;
  notes: string | null;
  tested_by: string | null;
  created_at: string;
}

export interface CreateBatteryTestRequest {
  test_date: string;
  result: BatteryTestResult;
  swelling_status?: SwellingStatus;
  capacity_measured?: number;
  internal_resistance?: number;
  cell_voltages?: number[];
  notes?: string;
}
