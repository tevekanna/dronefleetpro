// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type {
  AuditLogDetail,
  AuditLogFiltersResponse,
  AuditLogListParams,
  AuditLogListResponse,
} from '../types/auditLog';

const BASE = '/audit-log';

function toQuery(params: AuditLogListParams): string {
  const parts: string[] = [];
  if (params.from) parts.push(`from=${params.from}`);
  if (params.to) parts.push(`to=${params.to}`);
  if (params.user_id !== undefined) parts.push(`user_id=${params.user_id}`);
  if (params.table_name) parts.push(`table_name=${encodeURIComponent(params.table_name)}`);
  if (params.action) parts.push(`action=${params.action}`);
  if (params.search) parts.push(`search=${encodeURIComponent(params.search)}`);
  if (params.page !== undefined) parts.push(`page=${params.page}`);
  if (params.per_page !== undefined) parts.push(`per_page=${params.per_page}`);
  return parts.length ? `?${parts.join('&')}` : '';
}

export function listAuditLog(params: AuditLogListParams): Promise<AuditLogListResponse> {
  return apiClient.get<AuditLogListResponse>(`${BASE}${toQuery(params)}`);
}

export function getAuditLogEntry(id: number): Promise<AuditLogDetail> {
  return apiClient.get<AuditLogDetail>(`${BASE}/${id}`);
}

export function getAuditLogFilters(): Promise<AuditLogFiltersResponse> {
  return apiClient.get<AuditLogFiltersResponse>(`${BASE}/filters`);
}
