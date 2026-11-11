// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface BatchImportRow {
  serial_number: string;
  status: 'new' | 'duplicate' | 'invalid';
  suggested_compatible_model: string | null;
  existing_id: number | null;
  /** Aircraft only: SSID/password from a DJI Mimo QR or raw_qr from a Tello.
   *  Round-tripped to the commit endpoint as-is. */
  device_metadata?: unknown;
  /** Aircraft only: TRUE when the QR didn't yield a real SN (e.g. Tello).
   *  Surfaces a warning row in the preview UI. */
  needs_manual_sn?: boolean;
  /** Pre-fills the preview's Manufacturer input when the source CSV
   *  carried a manufacturer column (Standard CSV format). */
  manufacturer?: string | null;
  /** Pre-fills the preview's Model / Compatible Model input when the
   *  source CSV carried a model column (Standard CSV format). */
  model?: string | null;
}

export interface BatchImportPreview {
  entity_type: string;
  /** Total non-header rows the parser scanned. */
  total_rows: number;
  /** Valid SNs the parser kept — equal to `rows.length`. */
  total_parsed: number;
  /** Within-file duplicates dropped (preserving first occurrence). */
  duplicates_removed: number;
  rows: BatchImportRow[];
  /** Per-row warnings (Tello placeholder SN etc.). Surface inline. */
  warnings: string[];
  /** Per-row rejection reasons (URL, JSON, emoji, format mismatch).
   *  Show in a collapsible section so the modal isn't flooded. */
  errors: string[];
  /** Machine-readable format slug (e.g. "binary_eye_v1"). */
  source_format: string | null;
  /** Human-readable format label ("Binary Eye export"). */
  source_format_label: string | null;
}

export interface BatchCommitRow {
  serial_number: string;
  compatible_model?: string | null;
  model?: string | null;
  /** Aircraft only: when `model` is a new catalog entry, this seeds the
   *  auto-created aircraft_models row's manufacturer. Ignored if the model
   *  already exists. */
  manufacturer?: string | null;
  nickname?: string | null;
  max_cycles?: number | null;
  /** Aircraft only: pass-through of preview-side metadata (DJI Mimo
   *  SSID/password or Tello raw_qr). Backend persists as-is. */
  device_metadata?: unknown;
  /** Aircraft only: TRUE for Tello-style placeholder SN rows. */
  needs_manual_sn?: boolean;
}

export interface BatchCommitRequest {
  entity_type: string;
  rows: BatchCommitRow[];
}

export interface BatchCommitResult {
  created: number;
  skipped_duplicates: number;
  entity_type: string;
}

function getAuthHeaders(): Record<string, string> {
  const token = localStorage.getItem('access_token');
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export async function previewBatchImport(
  file: File,
  entityType: string
): Promise<BatchImportPreview> {
  const formData = new FormData();
  formData.append('file', file);
  formData.append('entity_type', entityType);

  // Vite dev proxy does not forward multipart/form-data correctly —
  // use direct backend URL in dev, relative path (nginx) in production.
  // window.location.hostname keeps fetch on the same host the frontend
  // was loaded from, so LAN-IP access (e.g. 192.168.x.x:5173) works the
  // same as localhost.
  const uploadUrl = import.meta.env.DEV
    ? `${window.location.protocol}//${window.location.hostname}:3000/api/v1/fleet/batch-import`
    : '/api/v1/fleet/batch-import';
  const res = await fetch(uploadUrl, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: formData,
  });

  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? `Upload failed: ${res.status}`);
  }
  return res.json();
}

export async function commitBatchImport(
  request: BatchCommitRequest
): Promise<BatchCommitResult> {
  const res = await fetch('/api/v1/fleet/batch-commit', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...getAuthHeaders(),
    },
    body: JSON.stringify(request),
  });

  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? `Commit failed: ${res.status}`);
  }
  return res.json();
}
