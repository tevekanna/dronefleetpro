// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export type PurgeTarget = 'flights' | 'aircraft' | 'batteries' | 'controllers' | 'pilots' | 'incidents';

export interface PurgeResult {
  deleted: number;
  table: string;
}

/** One affected table in a retention impact assessment. */
export interface RetentionDetail {
  table: string;
  column: string;
  count: number;
  /** "deleted" or "identity_stripped", possibly with an explanatory suffix
   *  (pilot qualifications carry one) — match on the prefix, not equality. */
  effect: string;
}

/** Response of GET /admin/purge/:target/preflight (EASA UAS.SPEC.050(1)(g)). */
export interface RetentionImpact {
  target: string;
  total_rows: number;
  within_retention: number;
  degraded_records: number;
  retention_years: number;
  details: RetentionDetail[];
}

/** Confirmation body required when a purge touches protected records. */
export interface PurgeConfirmation {
  acknowledged: number;
  reason: string;
}

export async function fetchPurgePreflight(target: PurgeTarget): Promise<RetentionImpact> {
  return apiClient.get<RetentionImpact>(`/admin/purge/${target}/preflight`);
}

export async function purgeData(
  target: PurgeTarget,
  confirmation?: PurgeConfirmation,
): Promise<PurgeResult> {
  return apiClient.delete<PurgeResult>(`/admin/purge/${target}`, confirmation);
}
