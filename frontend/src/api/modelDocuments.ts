// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient, getToken } from './client';

export type DocumentCategory = 'instruction_manual' | 'maintenance_manual' | 'other';

export interface ModelDocument {
  id: number;
  model_id: number;
  category: DocumentCategory;
  /// Operator-supplied label. Required server-side when category='other'
  /// (where multiple docs per model coexist and we need to tell them apart);
  /// NULL for the manual categories.
  title: string | null;
  original_filename: string;
  size_bytes: number;
  mime_type: string;
  uploaded_by: number;
  uploaded_at: string;
}

export interface UploadOptions {
  /// Required when category='other'. Server rejects 400 if missing or empty.
  /// Forbidden for manual categories.
  title?: string;
  /// Only valid when category='other'. Targets a specific existing doc for
  /// replacement. Without this, an 'other' upload always inserts a new row.
  replaceDocId?: number;
}

export async function fetchModelDocuments(modelId: number): Promise<ModelDocument[]> {
  return apiClient.get<ModelDocument[]>(`/aircraft-models/${modelId}/documents`);
}

/// Multipart upload mirroring ImportFlightLogModal's pattern: build FormData,
/// add the JWT Authorization header, target the dev-only direct backend URL
/// when import.meta.env.DEV (Vite proxy doesn't forward multipart cleanly).
/// In production the request goes through nginx via the relative path.
export async function uploadModelDocument(
  modelId: number,
  category: DocumentCategory,
  file: File,
  options?: UploadOptions,
): Promise<ModelDocument> {
  const formData = new FormData();
  formData.append('category', category);
  formData.append('file', file);
  if (options?.title !== undefined) {
    formData.append('title', options.title);
  }
  if (options?.replaceDocId !== undefined) {
    formData.append('replace_doc_id', String(options.replaceDocId));
  }

  const token = getToken();
  const path = `/api/v1/aircraft-models/${modelId}/documents`;
  const uploadUrl = import.meta.env.DEV
    ? `${window.location.protocol}//${window.location.hostname}:3000${path}`
    : path;

  const res = await fetch(uploadUrl, {
    method: 'POST',
    headers: token ? { Authorization: `Bearer ${token}` } : {},
    body: formData,
  });
  const data = await res.json().catch(() => null);
  if (!res.ok) {
    throw new Error(
      (data && (data.error as string | undefined)) ?? `Upload failed: ${res.status}`,
    );
  }
  return data as ModelDocument;
}

export async function deleteModelDocument(modelId: number, docId: number): Promise<void> {
  return apiClient.delete<void>(`/aircraft-models/${modelId}/documents/${docId}`);
}

/// Authenticated PDF download. Mirrors `downloadCsv` from api/fleetStats.ts —
/// fetch with JWT, parse the Content-Disposition filename, build a blob URL,
/// trigger the anchor click, and revoke. Necessary because <a href="..."> with
/// no auth header would just receive a 401 from the backend.
export async function downloadModelDocument(
  modelId: number,
  docId: number,
  fallbackFilename: string,
): Promise<void> {
  const token = getToken();
  const res = await fetch(
    `/api/v1/aircraft-models/${modelId}/documents/${docId}/download`,
    {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    },
  );
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(body || `Download failed: ${res.status}`);
  }
  const disposition = res.headers.get('Content-Disposition') ?? '';
  const match = /filename="([^"]+)"/.exec(disposition);
  const filename = match?.[1] ?? fallbackFilename;
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

