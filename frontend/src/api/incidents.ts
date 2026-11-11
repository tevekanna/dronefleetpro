// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient, getToken } from './client';

export interface Incident {
  id: number;
  org_id: number;
  flight_id: number | null;
  pilot_id: number | null;
  aircraft_id: number | null;
  occurred_at: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  category: string;
  location_description: string | null;
  description: string;
  immediate_action: string | null;
  root_cause: string | null;
  corrective_action: string | null;
  reported_to_authority: boolean;
  authority_ref: string | null;
  status: string;
  /** Annex VI occurrence code (e.g. "VI.1.4.1"), or null when unclassified. */
  occurrence_code: string | null;
  /** Start of the 72-hour clock under Art. 4(7) Reg (EU) No 376/2014 —
   *  runs from organisational awareness, NOT from occurred_at. */
  became_aware_at: string | null;
  reported_at: string | null;
  reporting_delay_reason: string | null;
  created_at: string;
  updated_at: string;
  created_by_user_id: number | null;
}

export type ReportingState =
  | 'not_applicable'
  | 'undetermined'
  | 'unclassified'
  | 'awaiting_awareness'
  | 'reported'
  | 'due'
  | 'overdue';

/** Computed occurrence-reporting status, embedded by the backend in every
 *  incident list row and in the detail response. deadline_at is only ever
 *  non-null when applicable is true AND an occurrence code is set — the UI
 *  must never render a deadline, countdown or urgency treatment otherwise. */
export interface ReportingStatus {
  state: ReportingState;
  applicable: boolean | null;
  deadline_at: string | null;
  hours_remaining: number | null;
  record_incomplete: boolean;
}

/** One row of GET /incidents: the incident fields flattened + reporting. */
export interface IncidentWithReporting extends Incident {
  reporting: ReportingStatus;
}

export type IncidentAttachmentKind =
  | 'ardupilot_bin'
  | 'ardupilot_tlog'
  | 'log'
  | 'document'
  | 'image'
  | 'other';

export interface IncidentAttachment {
  id: number;
  incident_id: number;
  kind: IncidentAttachmentKind;
  original_filename: string;
  size_bytes: number;
  mime_type: string;
  uploaded_by: number;
  uploaded_at: string;
}

export interface IncidentNote {
  id: number;
  incident_id: number;
  note: string;
  created_by: number | null;
  created_at: string;
}

export interface IncidentDetail {
  incident: Incident;
  reporting: ReportingStatus;
  attachments: IncidentAttachment[];
  notes: IncidentNote[];
}

export interface CreateIncidentRequest {
  occurred_at: string;
  severity: string;
  category: string;
  description: string;
  flight_id?: number;
  pilot_id?: number;
  aircraft_id?: number;
  location_description?: string;
  immediate_action?: string;
  reported_to_authority?: boolean;
  occurrence_code?: string;
  became_aware_at?: string;
}

/** Payload for editing the occurrence-classification fields. All four are
 *  tri-state on the backend (omitted = keep, explicit null = clear), so a
 *  cleared value MUST be sent as null — never dropped from the payload. */
export interface UpdateOccurrenceReportingRequest {
  occurrence_code: string | null;
  became_aware_at: string | null;
  reported_at: string | null;
  reporting_delay_reason: string | null;
}

export async function fetchIncidents(): Promise<IncidentWithReporting[]> {
  return apiClient.get('/incidents');
}

export async function createIncident(data: CreateIncidentRequest): Promise<Incident> {
  return apiClient.post('/incidents', data);
}

export async function updateIncident(id: number, data: object): Promise<Incident> {
  return apiClient.put(`/incidents/${id}`, data);
}

export async function deleteIncident(id: number): Promise<void> {
  return apiClient.delete(`/incidents/${id}`);
}

export async function fetchIncidentDetail(id: number): Promise<IncidentDetail> {
  return apiClient.get(`/incidents/${id}`);
}

export async function addIncidentNote(id: number, note: string): Promise<IncidentNote> {
  return apiClient.post(`/incidents/${id}/notes`, { note });
}

export async function deleteIncidentAttachment(incidentId: number, attId: number): Promise<void> {
  return apiClient.delete(`/incidents/${incidentId}/attachments/${attId}`);
}

/// Multipart upload mirroring uploadOrgLogo: raw fetch + JWT header + dev-only
/// direct backend URL (the Vite proxy does not forward multipart cleanly).
/// apiClient is unusable here because it forces application/json.
export async function uploadIncidentAttachment(incidentId: number, file: File): Promise<IncidentAttachment> {
  const formData = new FormData();
  formData.append('file', file);

  const token = getToken();
  const path = `/api/v1/incidents/${incidentId}/attachments`;
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
  return data as IncidentAttachment;
}

/// The download endpoint requires the JWT, so a plain <a href> cannot carry
/// the Authorization header. Fetch the blob with the token (dev-direct or
/// proxy URL, mirroring the upload path), then trigger a client-side download.
export async function downloadIncidentAttachment(
  incidentId: number,
  att: IncidentAttachment,
): Promise<void> {
  const token = getToken();
  const path = `/api/v1/incidents/${incidentId}/attachments/${att.id}/download`;
  const downloadUrl = import.meta.env.DEV
    ? `${window.location.protocol}//${window.location.hostname}:3000${path}`
    : path;

  const res = await fetch(downloadUrl, {
    method: 'GET',
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (!res.ok) {
    const data = await res.json().catch(() => null);
    throw new Error((data && (data.error as string | undefined)) ?? `Download failed: ${res.status}`);
  }

  const blob = await res.blob();
  const objectUrl = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = objectUrl;
  a.download = att.original_filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(objectUrl);
}
