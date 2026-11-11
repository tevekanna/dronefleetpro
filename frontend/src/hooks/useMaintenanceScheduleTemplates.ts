// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import * as api from '../api/maintenanceScheduleTemplates';
import type {
  CreateTemplateInput,
  ListTemplatesParams,
  SeedDefaultsRequest,
  SyncTemplateRequest,
  UpdateTemplateInput,
} from '../types/maintenanceScheduleTemplate';

const QK = ['maintenance-schedule-templates'] as const;

export function useScheduleTemplates(params?: ListTemplatesParams) {
  return useQuery({
    queryKey: [...QK, params],
    queryFn: () => api.fetchTemplates(params),
  });
}

export function useCreateScheduleTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateTemplateInput) => api.createTemplate(input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QK });
    },
  });
}

export function useUpdateScheduleTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, input }: { id: number; input: UpdateTemplateInput }) =>
      api.updateTemplate(id, input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QK });
    },
  });
}

export function useDeleteScheduleTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.deleteTemplate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QK });
    },
  });
}

export function usePermanentDeleteScheduleTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.permanentDeleteTemplate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QK });
      qc.invalidateQueries({ queryKey: ['maintenance-schedules'] });
    },
  });
}

export function useSyncScheduleTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, request }: { id: number; request: SyncTemplateRequest }) =>
      api.syncTemplate(id, request),
    onSuccess: () => {
      // Templates didn't change but the per-aircraft schedules did.
      qc.invalidateQueries({ queryKey: QK });
      qc.invalidateQueries({ queryKey: ['maintenance-schedules'] });
    },
  });
}

export function useSeedDefaultSchedules() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: SeedDefaultsRequest) => api.seedDefaultSchedules(request),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['maintenance-schedules'] });
    },
  });
}
