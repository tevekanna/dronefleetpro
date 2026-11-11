// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useCallback, useEffect, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import { AuditDetailDrawer } from '../components/audit/AuditDetailDrawer';
import { AuditFilterBar } from '../components/audit/AuditFilterBar';
import { AuditLogTable } from '../components/audit/AuditLogTable';
import { Pagination } from '../components/shared/Pagination';
import { useAuditLogList } from '../hooks/useAuditLog';
import type { AuditAction, AuditLogListParams } from '../types/auditLog';
import styles from './AuditLogPage.module.css';

const DEFAULT_PER_PAGE = 50;
const VALID_ACTIONS: ReadonlyArray<AuditAction> = [
  'INSERT',
  'UPDATE',
  'DELETE',
  'EXPORT',
  'IMPORT',
  'PURGE',
];

function parseAction(raw: string | null): AuditAction | undefined {
  if (!raw) return undefined;
  return (VALID_ACTIONS as ReadonlyArray<string>).includes(raw)
    ? (raw as AuditAction)
    : undefined;
}

function parseIntParam(raw: string | null): number | undefined {
  if (!raw) return undefined;
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : undefined;
}

function paramsFromURL(sp: URLSearchParams): AuditLogListParams {
  return {
    from: sp.get('from') || undefined,
    to: sp.get('to') || undefined,
    user_id: parseIntParam(sp.get('user_id')),
    table_name: sp.get('table_name') || undefined,
    action: parseAction(sp.get('action')),
    search: sp.get('search') || undefined,
    page: parseIntParam(sp.get('page')) ?? 1,
    per_page: parseIntParam(sp.get('per_page')) ?? DEFAULT_PER_PAGE,
  };
}

const KNOWN_PARAMS = [
  'from',
  'to',
  'user_id',
  'table_name',
  'action',
  'search',
  'page',
  'per_page',
];

function writeParamsToURL(
  sp: URLSearchParams,
  next: AuditLogListParams,
  entry: number | null,
): URLSearchParams {
  const out = new URLSearchParams();
  if (next.from) out.set('from', next.from);
  if (next.to) out.set('to', next.to);
  if (next.user_id !== undefined) out.set('user_id', String(next.user_id));
  if (next.table_name) out.set('table_name', next.table_name);
  if (next.action) out.set('action', next.action);
  if (next.search) out.set('search', next.search);
  if (next.page !== undefined && next.page !== 1) out.set('page', String(next.page));
  if (next.per_page !== undefined && next.per_page !== DEFAULT_PER_PAGE) {
    out.set('per_page', String(next.per_page));
  }
  if (entry !== null) out.set('entry', String(entry));
  // preserve unknown params for future extensibility
  sp.forEach((value, key) => {
    if (!out.has(key) && key !== 'entry' && !KNOWN_PARAMS.includes(key)) {
      out.set(key, value);
    }
  });
  return out;
}

export function AuditLogPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const filters = useMemo(() => paramsFromURL(searchParams), [searchParams]);
  const entryId = parseIntParam(searchParams.get('entry')) ?? null;

  const { data, isLoading, isFetching, error } = useAuditLogList(filters);

  const update = useCallback(
    (
      nextFilters: AuditLogListParams,
      nextEntry: number | null | 'preserve' = 'preserve',
    ) => {
      const entry = nextEntry === 'preserve' ? entryId : nextEntry;
      setSearchParams(writeParamsToURL(searchParams, nextFilters, entry), { replace: true });
    },
    [entryId, searchParams, setSearchParams],
  );

  const onFiltersChange = useCallback(
    (patch: Partial<AuditLogListParams>) => {
      // Read live URL state inside the callback so concurrent debounced patches
      // (From + To date inputs) don't race on stale captures. Changing any filter
      // also resets to page 1.
      const current = paramsFromURL(new URLSearchParams(window.location.search));
      update({ ...current, ...patch, page: 1 });
    },
    [update],
  );

  const onReset = useCallback(() => {
    update({ page: 1, per_page: filters.per_page }, null);
  }, [update, filters.per_page]);

  const onPageChange = useCallback(
    (page: number) => update({ ...filters, page }),
    [update, filters],
  );

  const onPerPageChange = useCallback(
    (per_page: number) => update({ ...filters, per_page, page: 1 }),
    [update, filters],
  );

  const onRowClick = useCallback(
    (id: number) => update(filters, id),
    [update, filters],
  );

  const onDrawerClose = useCallback(() => update(filters, null), [update, filters]);

  // Keep window scroll position stable between pages
  useEffect(() => {
    window.scrollTo({ top: 0 });
  }, [filters.page]);

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>Audit Log</h1>
        <span className={styles.description}>
          Immutable record of all changes, retained per EASA UAS.SPEC 4.2.3.
        </span>
      </div>

      <AuditFilterBar
        filters={filters}
        onChange={onFiltersChange}
        onReset={onReset}
      />

      {error && (
        <div className={styles.errorMsg}>
          Failed to load audit log: {(error as Error).message}
        </div>
      )}

      <AuditLogTable
        items={data?.items ?? []}
        selectedId={entryId}
        onRowClick={onRowClick}
        isLoading={isLoading || (isFetching && (data?.items.length ?? 0) === 0)}
      />

      {data && data.total > 0 && (
        <Pagination
          page={data.page}
          totalPages={data.total_pages}
          perPage={data.per_page}
          total={data.total}
          onPageChange={onPageChange}
          onPerPageChange={onPerPageChange}
          itemLabel="entries"
        />
      )}

      <AuditDetailDrawer
        isOpen={entryId !== null}
        entryId={entryId}
        onClose={onDrawerClose}
      />
    </div>
  );
}
