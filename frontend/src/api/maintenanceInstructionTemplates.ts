// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import {
  REQUIRES_NEW_REVISION,
  type TemplateListItem,
  type TemplateDetail,
  type CreateTemplateRequest,
  type CreateTemplateResponse,
  type EditTemplateRequest,
  type RevisionResponse,
} from '../types/maintenanceInstructionTemplate';

export async function fetchInstructionTemplates(filters: {
  aircraft_model_id?: number;
} = {}): Promise<TemplateListItem[]> {
  const params = new URLSearchParams();
  if (filters.aircraft_model_id !== undefined) {
    params.set('aircraft_model_id', String(filters.aircraft_model_id));
  }
  const qs = params.toString();
  return apiClient.get(qs
    ? `/maintenance-instruction-templates?${qs}`
    : '/maintenance-instruction-templates');
}

export async function fetchInstructionTemplateDetail(id: number): Promise<TemplateDetail> {
  return apiClient.get(`/maintenance-instruction-templates/${id}`);
}

export async function createInstructionTemplate(
  body: CreateTemplateRequest,
): Promise<CreateTemplateResponse> {
  return apiClient.post('/maintenance-instruction-templates', body);
}

export async function deactivateInstructionTemplate(id: number): Promise<void> {
  return apiClient.patch(`/maintenance-instruction-templates/${id}`, { is_active: false });
}

/**
 * Edit a template's items in place. Rejects with an Error whose message is
 * `requires_new_revision` (see {@link isRequiresNewRevision}) when the template
 * has recorded completions and must instead be revised.
 */
export async function updateInstructionTemplateItems(
  id: number,
  body: EditTemplateRequest,
): Promise<TemplateDetail> {
  return apiClient.put(`/maintenance-instruction-templates/${id}/items`, body);
}

/** Create the next revision of a template, re-pointing linked schedules. */
export async function createInstructionTemplateRevision(
  id: number,
  body: EditTemplateRequest,
): Promise<RevisionResponse> {
  return apiClient.post(`/maintenance-instruction-templates/${id}/revisions`, body);
}

/**
 * True when an error thrown by {@link updateInstructionTemplateItems} signals
 * that the edit must go through the new-revision path. The API client surfaces
 * the backend's machine-readable code as the Error message.
 */
export function isRequiresNewRevision(e: unknown): boolean {
  return e instanceof Error && e.message === REQUIRES_NEW_REVISION;
}
