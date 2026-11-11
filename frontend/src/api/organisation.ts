// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient, getToken } from './client';

export type LogoVariant = 'light' | 'dark';

export interface LogoMeta {
  data_url: string;
  mime_type: string;
  size_bytes: number;
  original_filename: string | null;
  updated_at: string;
}

export interface OrgBranding {
  light: LogoMeta | null;
  dark: LogoMeta | null;
}

export async function fetchOrgBranding(): Promise<OrgBranding> {
  return apiClient.get<OrgBranding>('/organisation/branding');
}

/// Multipart upload mirroring uploadModelDocument: raw fetch + JWT header +
/// dev-only direct backend URL (the Vite proxy does not forward multipart
/// cleanly). apiClient is unusable here because it forces application/json.
export async function uploadOrgLogo(variant: LogoVariant, file: File): Promise<void> {
  const formData = new FormData();
  formData.append('file', file);

  const token = getToken();
  const path = `/api/v1/organisation/logo/${variant}`;
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
    throw new Error((data && (data.error as string | undefined)) ?? `Upload failed: ${res.status}`);
  }
}

export async function deleteOrgLogo(variant: LogoVariant): Promise<void> {
  return apiClient.delete<void>(`/organisation/logo/${variant}`);
}
