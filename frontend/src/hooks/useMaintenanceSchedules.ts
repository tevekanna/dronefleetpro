// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  completeSchedule,
  createSchedule,
  deactivateSchedule,
  getDashboard,
  getSchedule,
  listSchedules,
  updateSchedule,
} from '../api/maintenanceSchedules';
import type {
  CompleteSchedulePayload,
  CreateSchedulePayload,
  ListSchedulesParams,
  UpdateSchedulePayload,
} from '../types/maintenanceSchedule';

const LIST_KEY = ['maintenance-schedules'] as const;
const DETAIL_KEY = (id: number) => ['maintenance-schedules', id] as const;
const DASHBOARD_KEY = ['maintenance-schedules', 'dashboard'] as const;

function invalidateAll(qc: ReturnType<typeof useQueryClient>): void {
  qc.invalidateQueries({ queryKey: LIST_KEY });
  qc.invalidateQueries({ queryKey: DASHBOARD_KEY });
}

export function useMaintenanceSchedules(params?: ListSchedulesParams) {
  return useQuery({
    queryKey: [...LIST_KEY, params ?? {}],
    queryFn: () => listSchedules(params),
  });
}

export function useMaintenanceSchedule(id: number | null) {
  return useQuery({
    queryKey: id ? DETAIL_KEY(id) : ['maintenance-schedules', 'none'],
    queryFn: () => {
      if (id === null) throw new Error('schedule id required');
      return getSchedule(id);
    },
    enabled: id !== null,
  });
}

export function useMaintenanceDashboard() {
  return useQuery({
    queryKey: DASHBOARD_KEY,
    queryFn: getDashboard,
    staleTime: 30 * 1000,
  });
}

export function useCreateMaintenanceSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (payload: CreateSchedulePayload) => createSchedule(payload),
    onSuccess: () => invalidateAll(qc),
  });
}

export function useUpdateMaintenanceSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: number; payload: UpdateSchedulePayload }) =>
      updateSchedule(args.id, args.payload),
    onSuccess: (_data, args) => {
      invalidateAll(qc);
      qc.invalidateQueries({ queryKey: DETAIL_KEY(args.id) });
    },
  });
}

export function useDeactivateMaintenanceSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => deactivateSchedule(id),
    onSuccess: () => invalidateAll(qc),
  });
}

export function useCompleteMaintenanceSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { id: number; payload: CompleteSchedulePayload }) =>
      completeSchedule(args.id, args.payload),
    onSuccess: (_data, args) => {
      invalidateAll(qc);
      qc.invalidateQueries({ queryKey: DETAIL_KEY(args.id) });
      qc.invalidateQueries({ queryKey: ['maintenance'] });
    },
  });
}
