// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type {
  ConflictStrategy,
  DataTransferJob,
  EntityGroup,
  ExportPreviewResponse,
  ImportSummary,
} from '../types/dataTransfer';

export async function previewExport(entities: EntityGroup[] | 'all'): Promise<ExportPreviewResponse> {
  const param = entities === 'all' ? 'all' : entities.join(',');
  return apiClient.get(`/data/export/preview?entities=${encodeURIComponent(param)}`);
}

/**
 * Triggers an export and returns the raw Response so the caller can pull
 * out the Content-Disposition filename and stream the body to a Blob.
 * The apiClient JSON helper isn't a fit here — the response is a file
 * payload, not a JSON envelope.
 *
 * Uses the relative `/api/v1/...` path; Vite proxies in dev, nginx in
 * prod. The `import.meta.env.DEV` localhost workaround that exists on
 * other endpoints is only for multipart uploads, which this is not.
 */
export async function exportData(entities: EntityGroup[] | 'all'): Promise<Response> {
  const token = localStorage.getItem('access_token');
  const body = JSON.stringify({ entities: entities === 'all' ? 'all' : entities });
  const res = await fetch('/api/v1/data/export', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? 'Export failed');
  }
  return res;
}

export async function listRecentJobs(limit = 10): Promise<DataTransferJob[]> {
  return apiClient.get(`/data/jobs?limit=${limit}`);
}

export async function previewImport(
  document: unknown,
  strategy: ConflictStrategy = 'skip',
  skipGroups: EntityGroup[] = [],
): Promise<ImportSummary> {
  return apiClient.post('/data/import/preview', {
    document,
    strategy,
    skip_groups: skipGroups,
  });
}

export async function applyImport(
  document: unknown,
  strategy: ConflictStrategy = 'skip',
  skipGroups: EntityGroup[] = [],
): Promise<ImportSummary> {
  return apiClient.post('/data/import', {
    document,
    strategy,
    skip_groups: skipGroups,
  });
}
