// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import {
  getAircraftByModel,
  getBatteryCycleDistribution,
  getBatteryModelComparison,
  getBatteryStatsDetail,
  getControllerStats,
  getFleetDashboard,
  getFleetStatsOverview,
} from '../api/fleetStats';
import { fetchMaintenance, fetchDefects } from '../api/maintenance';
import { listSchedules } from '../api/maintenanceSchedules';
import { fetchAircraftModelStats } from '../api/aircraftModels';
import { fetchAircraftBreakdowns } from '../api/aircraftDetail';
import { fetchModelDocuments } from '../api/modelDocuments';
import {
  fetchAllOrgChecklists,
  fetchChecklist,
  fetchModelChecklists,
} from '../api/checklists';

const STALE = 2 * 60 * 1000;

export function useFleetStatsOverview() {
  return useQuery({
    queryKey: ['fleet-stats', 'overview'],
    queryFn: getFleetStatsOverview,
    staleTime: STALE,
  });
}

export function useFleetDashboard() {
  return useQuery({
    queryKey: ['fleet-stats', 'dashboard'],
    queryFn: getFleetDashboard,
    staleTime: STALE,
  });
}

export function useAircraftByModel(enabled = true) {
  return useQuery({
    queryKey: ['fleet-stats', 'aircraft-by-model'],
    queryFn: getAircraftByModel,
    staleTime: STALE,
    enabled,
  });
}

export function useBatteryCycleDistribution(enabled = true) {
  return useQuery({
    queryKey: ['fleet-stats', 'battery-cycle-distribution'],
    queryFn: getBatteryCycleDistribution,
    staleTime: STALE,
    enabled,
  });
}

export function useBatteryStats(id: number | null) {
  return useQuery({
    queryKey: ['fleet-stats', 'battery', id],
    queryFn: () => {
      if (id === null) throw new Error('battery id required');
      return getBatteryStatsDetail(id);
    },
    enabled: id !== null,
    staleTime: STALE,
  });
}

export function useBatteryModelComparison(model: string | null) {
  return useQuery({
    queryKey: ['fleet-stats', 'model', model],
    queryFn: () => {
      if (model === null) throw new Error('model required');
      return getBatteryModelComparison(model);
    },
    enabled: model !== null,
    staleTime: STALE,
  });
}

export function useControllerStats(id: number | null) {
  return useQuery({
    queryKey: ['fleet-stats', 'controller', id],
    queryFn: () => {
      if (id === null) throw new Error('controller id required');
      return getControllerStats(id);
    },
    enabled: id !== null,
    staleTime: STALE,
  });
}

export function useAircraftModelStats(modelId: number | null) {
  return useQuery({
    queryKey: ['aircraft-model-stats', modelId],
    queryFn: () => {
      if (modelId === null) throw new Error('model id required');
      return fetchAircraftModelStats(modelId);
    },
    enabled: modelId !== null,
    staleTime: STALE,
  });
}

export function useModelDocuments(modelId: number | null) {
  return useQuery({
    queryKey: ['model-documents', modelId],
    queryFn: () => {
      if (modelId === null) throw new Error('model id required');
      return fetchModelDocuments(modelId);
    },
    enabled: modelId !== null,
    staleTime: STALE,
  });
}

export function useModelChecklists(modelId: number | null) {
  return useQuery({
    queryKey: ['model-checklists', modelId],
    queryFn: () => {
      if (modelId === null) throw new Error('model id required');
      return fetchModelChecklists(modelId);
    },
    enabled: modelId !== null,
    staleTime: STALE,
  });
}

export function useChecklist(checklistId: number | null) {
  return useQuery({
    queryKey: ['checklist', checklistId],
    queryFn: () => {
      if (checklistId === null) throw new Error('checklist id required');
      return fetchChecklist(checklistId);
    },
    enabled: checklistId !== null,
    staleTime: STALE,
  });
}

/// Used only by the "Copy from existing" picker in the create flow. Lazy-fetch
/// — keep `enabled` false until the picker actually mounts so the org-wide
/// fan-out doesn't run on every model detail visit.
export function useAllOrgChecklists(enabled: boolean) {
  return useQuery({
    queryKey: ['all-org-checklists'],
    queryFn: fetchAllOrgChecklists,
    enabled,
    staleTime: STALE,
  });
}

export function useAircraftBreakdowns(aircraftId: number | null) {
  return useQuery({
    queryKey: ['aircraft-breakdowns', aircraftId],
    queryFn: () => {
      if (aircraftId === null) throw new Error('aircraft id required');
      return fetchAircraftBreakdowns(aircraftId);
    },
    enabled: aircraftId !== null,
    staleTime: STALE,
  });
}

export function useAircraftMaintenance(aircraftId: number | null) {
  return useQuery({
    queryKey: ['aircraft-maintenance', aircraftId],
    queryFn: () => {
      if (aircraftId === null) throw new Error('aircraft id required');
      return fetchMaintenance({ aircraft_id: aircraftId });
    },
    enabled: aircraftId !== null,
    staleTime: STALE,
  });
}

export function useAircraftMaintenanceSchedules(aircraftId: number | null) {
  return useQuery({
    queryKey: ['aircraft-maintenance-schedules', aircraftId],
    queryFn: () => {
      if (aircraftId === null) throw new Error('aircraft id required');
      return listSchedules({ aircraft_id: aircraftId, include_inactive: false });
    },
    enabled: aircraftId !== null,
    staleTime: STALE,
  });
}

export function useAircraftDefects(aircraftId: number | null) {
  return useQuery({
    queryKey: ['aircraft-defects', aircraftId],
    queryFn: () => {
      if (aircraftId === null) throw new Error('aircraft id required');
      return fetchDefects({ aircraft_id: aircraftId });
    },
    enabled: aircraftId !== null,
    staleTime: STALE,
  });
}

export function useControllerMaintenance(controllerId: number | null) {
  return useQuery({
    queryKey: ['controller-maintenance', controllerId],
    queryFn: () => {
      if (controllerId === null) throw new Error('controller id required');
      return fetchMaintenance({ controller_id: controllerId });
    },
    enabled: controllerId !== null,
    staleTime: STALE,
  });
}
