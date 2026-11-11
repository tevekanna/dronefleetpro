// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type AuditAction =
  | 'INSERT'
  | 'UPDATE'
  | 'DELETE'
  | 'EXPORT'
  | 'IMPORT'
  | 'PURGE'
  | 'LOGIN_SUCCESS'
  | 'LOGIN_FAILURE'
  | 'LOGOUT'
  | 'TOKEN_REDEEMED';

export interface AuditLogListItem {
  id: number;
  action: AuditAction;
  table_name: string;
  record_id: number | null;
  user_id: number | null;
  user_code: string | null;
  user_name: string | null;
  ip_address: string | null;
  created_at: string;
  has_diff: boolean;
  /** null for INSERT/DELETE, [] for UPDATE without a meaningful diff. */
  changed_fields: string[] | null;
}

export interface AuditChangedField {
  field: string;
  before: unknown;
  after: unknown;
}

export interface AuditLogDetail extends Omit<AuditLogListItem, 'changed_fields'> {
  diff: { before?: unknown; after?: unknown } | null;
  changed_fields: AuditChangedField[] | null;
}

export interface AuditLogFiltersResponse {
  tables: string[];
  users: Array<{ id: number; name: string; user_code: string }>;
  actions: AuditAction[];
}

export interface AuditLogListParams {
  from?: string;
  to?: string;
  user_id?: number;
  table_name?: string;
  action?: AuditAction;
  search?: string;
  page?: number;
  per_page?: number;
}

export interface AuditLogListResponse {
  items: AuditLogListItem[];
  page: number;
  per_page: number;
  total: number;
  total_pages: number;
}
