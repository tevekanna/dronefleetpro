// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient, getToken } from './client';
import type {
  BatteryModelComparison,
  BatteryStatsDetail,
  ControllerStats,
  FleetStatsOverview,
} from '../types/fleetStats';
import type {
  AircraftByModelResponse,
  BatteryCycleDistribution,
  FleetDashboard,
} from '../types/fleetDashboard';

const BASE = '/fleet-stats';

export function getFleetStatsOverview(): Promise<FleetStatsOverview> {
  return apiClient.get<FleetStatsOverview>(`${BASE}/overview`);
}

export function getFleetDashboard(): Promise<FleetDashboard> {
  return apiClient.get<FleetDashboard>(`${BASE}/dashboard`);
}

export function getAircraftByModel(): Promise<AircraftByModelResponse> {
  return apiClient.get<AircraftByModelResponse>(`${BASE}/aircraft-by-model`);
}

export function getBatteryCycleDistribution(): Promise<BatteryCycleDistribution> {
  return apiClient.get<BatteryCycleDistribution>(`${BASE}/battery-cycle-distribution`);
}

export function getBatteryStatsDetail(id: number): Promise<BatteryStatsDetail> {
  return apiClient.get<BatteryStatsDetail>(`${BASE}/batteries/${id}`);
}

export function getBatteryModelComparison(model: string): Promise<BatteryModelComparison> {
  return apiClient.get<BatteryModelComparison>(
    `${BASE}/batteries/model/${encodeURIComponent(model)}`,
  );
}

export function getControllerStats(id: number): Promise<ControllerStats> {
  return apiClient.get<ControllerStats>(`${BASE}/controllers/${id}`);
}

/**
 * Fetch a CSV export and trigger a browser download. CSV streams through the
 * authenticated fetch path — not the anchor-download shortcut — so the JWT is
 * sent and non-200 responses surface as errors instead of downloaded error
 * pages.
 */
async function downloadCsv(path: string): Promise<void> {
  const token = getToken();
  const res = await fetch(`/api/v1${path}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(body || `CSV export failed: ${res.status}`);
  }
  const disposition = res.headers.get('Content-Disposition') ?? '';
  const match = /filename="([^"]+)"/.exec(disposition);
  const filename = match?.[1] ?? 'export.csv';
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

export function downloadBatteriesCsv(): Promise<void> {
  return downloadCsv(`${BASE}/export/batteries`);
}

export function downloadControllersCsv(): Promise<void> {
  return downloadCsv(`${BASE}/controllers/export`);
}
