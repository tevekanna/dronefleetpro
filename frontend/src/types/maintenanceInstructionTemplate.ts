// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface TemplateListItem {
  id: number;
  org_id: number;
  aircraft_model_id: number | null;
  name: string;
  description: string | null;
  revision: string;
  source: 'manufacturer' | 'custom';
  is_active: boolean;
  created_at: string;
  updated_at: string;
  item_count: number;
}

export interface ChecklistItemSummary {
  id: number;
  item_order: number;
  item_text: string;
  is_required: boolean;
  reference_manual: string | null;
}

export interface TemplateSection {
  section_name: string;
  section_order: number;
  items: ChecklistItemSummary[];
}

export interface TemplateDetail extends TemplateListItem {
  sections: TemplateSection[];
}

export interface CreateItemInput {
  item_text: string;
  is_required: boolean;
  reference_manual?: string;
  item_order: number;
}

export interface CreateSectionInput {
  section_name: string;
  section_order: number;
  items: CreateItemInput[];
}

export interface CreateTemplateRequest {
  name: string;
  aircraft_model_id: number | null;
  description?: string;
  revision?: string;
  sections: CreateSectionInput[];
}

export interface CreateTemplateResponse {
  id: number;
  name: string;
  sections: number;
  items: number;
}

/** Body for PUT /:id/items (edit in place) and POST /:id/revisions. */
export interface EditTemplateRequest {
  name?: string;
  description?: string;
  sections: CreateSectionInput[];
}

/** Response from POST /:id/revisions. */
export interface RevisionResponse {
  template: TemplateDetail;
  repointed_schedules: number;
}

/** Machine-readable 409 code returned when an in-place edit is refused. */
export const REQUIRES_NEW_REVISION = 'requires_new_revision';
