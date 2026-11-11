// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

/** One Annex VI occurrence category, verbatim regulatory text. Sections 1
 *  and 2 carry subsections; sections 3–8 have none. */
export interface OccurrenceCategory {
  /** Stable identifier, e.g. "VI.1.4.11" or "VI.6.10". */
  code: string;
  section: number;
  section_title: string;
  subsection: string | null;
  subsection_title: string | null;
  text: string;
}

export interface OccurrenceTaxonomyResponse {
  /** Source citation for the taxonomy — display near any picker. */
  source: string;
  categories: OccurrenceCategory[];
}

export async function fetchOccurrenceTaxonomy(): Promise<OccurrenceTaxonomyResponse> {
  return apiClient.get('/occurrence-taxonomy');
}
