// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { getAuditLogEntry, getAuditLogFilters, listAuditLog } from '../api/auditLog';
import type { AuditLogListParams } from '../types/auditLog';

export function useAuditLogList(params: AuditLogListParams) {
  return useQuery({
    queryKey: ['audit-log', params],
    queryFn: () => listAuditLog(params),
    placeholderData: keepPreviousData,
  });
}

export function useAuditLogEntry(id: number | null) {
  return useQuery({
    queryKey: ['audit-log', 'entry', id],
    queryFn: () => {
      if (id === null) throw new Error('entry id required');
      return getAuditLogEntry(id);
    },
    enabled: id !== null,
  });
}

export function useAuditLogFilters() {
  return useQuery({
    queryKey: ['audit-log', 'filters'],
    queryFn: getAuditLogFilters,
    staleTime: 5 * 60 * 1000,
  });
}
