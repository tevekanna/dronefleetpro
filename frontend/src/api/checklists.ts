// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import { fetchAircraftModels } from './aircraftModels';

export type ChecklistType = 'normal_ops' | 'qrh';
export type Responsibility = 'PF' | 'PM' | 'PF/PM' | 'N/A';

/// List endpoint shape: header + denormalised counts. Used by ChecklistsSection
/// and by the "Copy from existing" picker in the create flow.
export interface ChecklistSummary {
  id: number;
  model_id: number;
  type: ChecklistType;
  title: string;
  revision: string;
  revision_date: string | null;
  updated_at: string;
  section_count: number;
  item_count: number;
}

export interface ChecklistItem {
  id: number;
  section_id: number;
  label: string;
  value: string;
  responsibility: Responsibility;
  sort_order: number;
}

export interface ChecklistSection {
  id: number;
  checklist_id: number;
  title: string;
  sort_order: number;
  start_on_new_page: boolean;
  items: ChecklistItem[];
}

export interface Checklist {
  id: number;
  org_id: number;
  model_id: number;
  type: ChecklistType;
  title: string;
  revision: string;
  revision_date: string | null;
  created_by: number;
  updated_by: number | null;
  created_at: string;
  updated_at: string;
}

/// Full nested response from GET /checklists/:id and the POST/PUT responses.
/// Mirrors the Rust ChecklistFull which uses serde(flatten) for the parent
/// fields and appends `sections`.
export interface ChecklistFull extends Checklist {
  sections: ChecklistSection[];
}

/// Request body for POST and PUT. Server assigns IDs; sort_order is set by
/// the caller based on array position at save time.
export interface ChecklistInputBody {
  type: ChecklistType;
  title: string;
  revision?: string;
  revision_date?: string | null;
  sections: ChecklistSectionInput[];
}

export interface ChecklistSectionInput {
  title: string;
  sort_order: number;
  start_on_new_page: boolean;
  items: ChecklistItemInput[];
}

export interface ChecklistItemInput {
  label: string;
  value: string;
  responsibility: Responsibility;
  sort_order: number;
}

export function fetchModelChecklists(modelId: number): Promise<ChecklistSummary[]> {
  return apiClient.get<ChecklistSummary[]>(`/aircraft-models/${modelId}/checklists`);
}

export function fetchChecklist(checklistId: number): Promise<ChecklistFull> {
  return apiClient.get<ChecklistFull>(`/checklists/${checklistId}`);
}

/// Aggregate org-wide list, used by the "Copy from existing" picker. The
/// backend has no /api/v1/checklists list endpoint; we fan out one
/// per-model call instead. Acceptable because the model count per org is
/// small (typically < 30 — verified against production data).
export async function fetchAllOrgChecklists(): Promise<
  Array<ChecklistSummary & { model_name: string }>
> {
  const models = await fetchAircraftModels();
  const lists = await Promise.all(
    models.map(async (m) => {
      const summaries = await fetchModelChecklists(m.id);
      return summaries.map((s) => ({ ...s, model_name: m.name }));
    }),
  );
  return lists.flat();
}

export function createModelChecklist(
  modelId: number,
  body: ChecklistInputBody,
): Promise<ChecklistFull> {
  return apiClient.post<ChecklistFull>(`/aircraft-models/${modelId}/checklists`, body);
}

export function updateChecklist(
  checklistId: number,
  body: ChecklistInputBody,
): Promise<ChecklistFull> {
  return apiClient.put<ChecklistFull>(`/checklists/${checklistId}`, body);
}

export function deleteChecklist(checklistId: number): Promise<void> {
  return apiClient.delete<void>(`/checklists/${checklistId}`);
}
