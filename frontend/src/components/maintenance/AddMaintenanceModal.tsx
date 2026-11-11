// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useCallback, useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createMaintenance, saveMaintenanceCompletions } from '../../api/maintenance';
import { fetchFlights } from '../../api/flights';
import { fetchInstructionTemplates, fetchInstructionTemplateDetail } from '../../api/maintenanceInstructionTemplates';
import { showToast } from '../../store/toastStore';
import type { Aircraft } from '../../types/aircraft';
import type { Flight } from '../../types/flight';
import type { TemplateDetail, ChecklistItemSummary } from '../../types/maintenanceInstructionTemplate';
import { formatDuration } from '../flight-detail/flightFormatters';
import { IsoDateInput } from '../inputs/IsoDateInput';
import { formatAircraftLabel } from '../../utils/aircraftFormat';

/// Minimal controller shape the modal needs. Callers can pass either a full
/// `Controller` (from types/controller.ts) or a denormalized stats payload
/// (e.g. ControllerStats) — both satisfy this surface.
export interface ControllerTargetSummary {
  id: number;
  serial_number: string;
  equipment_id: string | null;
  model: string | null;
  /// Optional — when known, the flight picker scopes to flights flown with
  /// this controller's currently-paired aircraft. When undefined or null,
  /// flight linking is disabled.
  aircraft_id?: number | null;
}

export type AddMaintenanceTarget =
  | { kind: 'aircraft'; aircraft: Aircraft[]; preselectedAircraftId?: number }
  | { kind: 'controller'; controller: ControllerTargetSummary };

interface AddMaintenanceModalProps {
  target: AddMaintenanceTarget;
  onClose: () => void;
  onSuccess?: () => void;
}

const today = new Date().toISOString().slice(0, 10);

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0,0,0,0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1000,
};
const modalStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border-strong)',
  borderRadius: 'var(--radius-lg)',
  padding: 'var(--space-6)',
  width: '100%',
  maxWidth: '520px',
  boxShadow: 'var(--shadow-modal)',
  maxHeight: '90vh',
  overflowY: 'auto',
};
const fieldStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-1)',
  marginBottom: 'var(--space-4)',
};
const labelStyle: React.CSSProperties = {
  fontSize: '0.8rem',
  color: 'var(--color-text-secondary)',
  fontFamily: 'var(--font-label)',
};
const inputStyle: React.CSSProperties = {
  background: 'var(--color-bg-input)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  color: 'var(--color-text-primary)',
  fontSize: '0.9rem',
  fontFamily: 'var(--font-body)',
};
const monoStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '0.8rem',
};
const cancelBtnStyle: React.CSSProperties = {
  background: 'transparent',
  color: 'var(--color-text-secondary)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: 'pointer',
  fontSize: '0.85rem',
};
const submitBtnStyle = (pending: boolean): React.CSSProperties => ({
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: 'pointer',
  fontSize: '0.85rem',
  fontWeight: 500,
  opacity: pending ? 0.7 : 1,
});
const readonlyTargetCardStyle: React.CSSProperties = {
  background: 'var(--color-bg-input)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-3)',
  display: 'grid',
  gridTemplateColumns: 'auto 1fr',
  gap: 'var(--space-1) var(--space-3)',
  fontSize: '0.85rem',
};

function flightPickerLabel(f: Flight): string {
  const dt = f.flight_datetime ?? '';
  const date = dt ? dt.substring(0, 10) : '—';
  const time = dt ? dt.substring(11, 16) : '';
  const duration = formatDuration(f.duration_seconds);
  const context = f.pic_name || f.operation_name || f.mission_type || '—';
  const datetime = time ? `${date} ${time}` : date;
  return `${datetime} · ${duration} · ${context}`;
}

export function AddMaintenanceModal({ target, onClose, onSuccess }: AddMaintenanceModalProps) {
  const queryClient = useQueryClient();
  const initialAircraftId =
    target.kind === 'aircraft' && target.preselectedAircraftId !== undefined
      ? String(target.preselectedAircraftId)
      : '';
  const [aircraftId, setAircraftId] = useState(initialAircraftId);
  const [performedDate, setPerformedDate] = useState(today);
  const [flightId, setFlightId] = useState<string>('');
  const [createdBy, setCreatedBy] = useState('');
  const [remark, setRemark] = useState('');
  const [actionTaken, setActionTaken] = useState('');
  const [maintCategory, setMaintCategory] = useState('');
  const [maintType, setMaintType] = useState('');
  const [signOff, setSignOff] = useState('');
  const [error, setError] = useState('');

  // Checklist state — only used when aircraft kind and model_id is known
  const [checklistStatus, setChecklistStatus] = useState<Record<number, 'ok' | 'nok' | 'na' | 'pending'>>({});
  const [checklistNotes, setChecklistNotes] = useState<Record<number, string>>({});

  // Flight picker is scoped to: the selected aircraft (aircraft kind) OR the
  // controller's currently-paired aircraft (controller kind, fallback only).
  const flightScopeAircraftId =
    target.kind === 'aircraft' ? aircraftId : (target.controller.aircraft_id ?? '') || '';

  // Aircraft kind: reset flight selection whenever the selected aircraft
  // changes — flights are aircraft-scoped, so a stale flightId from a
  // different aircraft must never reach the submit payload.
  useEffect(() => {
    if (target.kind === 'aircraft') setFlightId('');
  }, [aircraftId, target.kind]);

  const selectedAircraft =
    target.kind === 'aircraft'
      ? (target.aircraft.find((a) => String(a.id) === aircraftId) ?? null)
      : null;
  const modelId = selectedAircraft?.model_id ?? null;

  // Fetch templates for this model — only when aircraft kind and model selected.
  const templateListQuery = useQuery({
    queryKey: ['instruction-templates-for-model', modelId],
    queryFn: () => fetchInstructionTemplates({ aircraft_model_id: modelId! }),
    enabled: target.kind === 'aircraft' && modelId !== null,
    staleTime: 60_000,
  });
  const matchedTemplate = templateListQuery.data?.[0] ?? null;

  // Fetch full detail (sections + items) only when a template is matched.
  const templateDetailQuery = useQuery({
    queryKey: ['instruction-template-detail', matchedTemplate?.id],
    queryFn: () => fetchInstructionTemplateDetail(matchedTemplate!.id),
    enabled: matchedTemplate !== null,
    staleTime: 60_000,
  });
  const templateDetail: TemplateDetail | null = templateDetailQuery.data ?? null;

  // When template detail arrives, set all items to 'pending' if not already set.
  useEffect(() => {
    if (!templateDetail) return;
    setChecklistStatus((prev) => {
      const next = { ...prev };
      for (const section of templateDetail.sections) {
        for (const item of section.items) {
          if (!(item.id in next)) next[item.id] = 'pending';
        }
      }
      return next;
    });
  }, [templateDetail]);

  // Reset checklist when aircraft changes.
  useEffect(() => {
    setChecklistStatus({});
    setChecklistNotes({});
  }, [aircraftId]);

  const setItemStatus = useCallback(
    (itemId: number, status: 'ok' | 'nok' | 'na') => {
      setChecklistStatus((prev) => ({ ...prev, [itemId]: status }));
    },
    [],
  );

  const mutation = useMutation({
    mutationFn: (data: object) => createMaintenance(data),
    onSuccess: async (record) => {
      // If a checklist template was completed, save the completions.
      if (templateDetail && matchedTemplate) {
        const completions = templateDetail.sections.flatMap((section) =>
          section.items.map((item: ChecklistItemSummary) => ({
            checklist_item_id: item.id,
            status: checklistStatus[item.id] ?? 'pending',
            note: checklistNotes[item.id]?.trim() || undefined,
            completed_by: createdBy.toUpperCase(),
          })),
        );
        try {
          const completionResult = await saveMaintenanceCompletions(record.id, {
            template_id: matchedTemplate.id,
            completions,
          });
          if (completionResult.defects_created > 0) {
            showToast(
              `${completionResult.defects_created} defect${completionResult.defects_created > 1 ? 's' : ''} reported automatically from NOK items.`,
              'info',
            );
          }
        } catch (e) {
          // Non-fatal: the maintenance record was created; completions failed.
          console.error('Failed to save checklist completions:', e);
        }
      }
      queryClient.invalidateQueries({ queryKey: ['maintenance'] });
      if (target.kind === 'controller') {
        queryClient.invalidateQueries({ queryKey: ['controller-maintenance', target.controller.id] });
      }
      onSuccess?.();
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const flightsQuery = useQuery({
    queryKey: ['flights', 'for-aircraft', flightScopeAircraftId],
    queryFn: () => fetchFlights({ aircraftId: Number(flightScopeAircraftId), perPage: 50 }),
    enabled: !!flightScopeAircraftId,
    staleTime: 60_000,
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (target.kind === 'aircraft' && !aircraftId) {
      setError('Aircraft is required');
      return;
    }
    if (!performedDate) {
      setError('Date is required');
      return;
    }
    if (!createdBy || createdBy.length !== 3) {
      setError('Created By must be exactly 3 uppercase letters');
      return;
    }
    if (remark.trim().length < 5) {
      setError('Remark must describe WHAT was observed (min 5 chars)');
      return;
    }
    if (actionTaken.trim().length < 5) {
      setError('Action Taken must describe WHAT was done (min 5 chars)');
      return;
    }
    if (!maintCategory) {
      setError('Category is required');
      return;
    }
    if (!maintType) {
      setError('Type is required');
      return;
    }
    if (!signOff || signOff.length !== 3) {
      setError('Sign Off must be exactly 3 uppercase letters');
      return;
    }
    if (createdBy.toUpperCase() === signOff.toUpperCase()) {
      setError('EASA OSO #3: Created By and Sign Off must differ (separation of duties)');
      return;
    }
    setError('');
    const targetIds =
      target.kind === 'aircraft'
        ? { aircraft_id: Number(aircraftId) }
        : { controller_id: target.controller.id };

    // Validate checklist: all required items must be ok/nok/na (not pending).
    if (templateDetail) {
      for (const section of templateDetail.sections) {
        for (const item of section.items) {
          if (item.is_required && (!checklistStatus[item.id] || checklistStatus[item.id] === 'pending')) {
            setError(`Section "${section.section_name}" item ${item.item_order}: required — mark OK, NOK, or N/A before saving.`);
            return;
          }
          if (checklistStatus[item.id] === 'nok' && !checklistNotes[item.id]?.trim()) {
            setError(`Section "${section.section_name}" item ${item.item_order}: NOK items require a note.`);
            return;
          }
        }
      }
    }

    mutation.mutate({
      ...targetIds,
      performed_date: performedDate,
      flight_id: flightId ? Number(flightId) : null,
      created_by: createdBy.toUpperCase(),
      remark: remark.trim(),
      action_taken: actionTaken.trim(),
      maint_category: maintCategory,
      maint_type: maintType,
      sign_off: signOff.toUpperCase(),
    });
  };

  const heading =
    target.kind === 'controller' ? 'Add Maintenance Event — Controller' : 'Add Maintenance Record';

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={(e) => e.stopPropagation()}>
        <h2
          style={{
            fontFamily: 'var(--font-display)',
            fontSize: '1.1rem',
            color: 'var(--color-text-primary)',
            marginBottom: 'var(--space-5)',
          }}
        >
          {heading}
        </h2>
        <p
          style={{
            fontSize: '0.75rem',
            color: 'var(--color-text-muted)',
            marginBottom: 'var(--space-4)',
          }}
        >
          EASA OSO #3 — All fields marked * are required
        </p>
        <form onSubmit={handleSubmit}>
          {target.kind === 'aircraft' ? (
            <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
              <div style={{ ...fieldStyle, flex: 1 }}>
                <label style={labelStyle}>Aircraft *</label>
                <select
                  style={inputStyle}
                  value={aircraftId}
                  onChange={(e) => setAircraftId(e.target.value)}
                  required
                >
                  <option value="">— Select —</option>
                  {target.aircraft.map((a) => (
                    <option key={a.id} value={a.id}>
                      {formatAircraftLabel(a)}
                    </option>
                  ))}
                </select>
              </div>
              <div style={{ ...fieldStyle, flex: 1 }}>
                <label style={labelStyle}>Date *</label>
                <IsoDateInput
                  style={inputStyle}
                  value={performedDate}
                  onChange={(e) => setPerformedDate(e.target.value)}
                  required
                />
              </div>
            </div>
          ) : (
            <>
              <div style={fieldStyle}>
                <label style={labelStyle}>Controller (target)</label>
                <div style={readonlyTargetCardStyle}>
                  <span style={{ color: 'var(--color-text-muted)' }}>Equipment</span>
                  <span style={monoStyle}>
                    {target.controller.equipment_id ?? target.controller.serial_number}
                  </span>
                  <span style={{ color: 'var(--color-text-muted)' }}>Model</span>
                  <span>{target.controller.model ?? '—'}</span>
                  <span style={{ color: 'var(--color-text-muted)' }}>Serial</span>
                  <span style={monoStyle}>{target.controller.serial_number}</span>
                </div>
              </div>
              <div style={fieldStyle}>
                <label style={labelStyle}>Date *</label>
                <IsoDateInput
                  style={inputStyle}
                  value={performedDate}
                  onChange={(e) => setPerformedDate(e.target.value)}
                  required
                />
              </div>
            </>
          )}

          <div style={fieldStyle}>
            <label style={labelStyle}>
              Flight (optional)
              {target.kind === 'controller' && (
                <span
                  style={{
                    color: 'var(--color-text-muted)',
                    marginLeft: 'var(--space-2)',
                    fontSize: '0.7rem',
                  }}
                >
                  — scoped to currently-paired aircraft
                </span>
              )}
            </label>
            <select
              style={inputStyle}
              value={flightId}
              onChange={(e) => setFlightId(e.target.value)}
              disabled={!flightScopeAircraftId}
            >
              <option value="">— No specific flight —</option>
              {flightsQuery.data?.flights?.map((item) => (
                <option key={item.id} value={item.id}>
                  {flightPickerLabel(item)}
                </option>
              ))}
            </select>
            {flightScopeAircraftId && flightsQuery.isLoading && (
              <small
                style={{
                  color: 'var(--color-text-muted)',
                  display: 'block',
                  marginTop: 'var(--space-1)',
                }}
              >
                Loading flights…
              </small>
            )}
            {flightScopeAircraftId && flightsQuery.isError && (
              <small
                style={{
                  color: 'var(--color-critical)',
                  display: 'block',
                  marginTop: 'var(--space-1)',
                }}
              >
                Failed to load flights
              </small>
            )}
            {flightScopeAircraftId &&
              !flightsQuery.isLoading &&
              flightsQuery.data &&
              flightsQuery.data.flights.length === 0 && (
                <small
                  style={{
                    color: 'var(--color-text-muted)',
                    display: 'block',
                    marginTop: 'var(--space-1)',
                  }}
                >
                  No flights yet for this aircraft
                </small>
              )}
            {target.kind === 'controller' && !flightScopeAircraftId && (
              <small
                style={{
                  color: 'var(--color-text-muted)',
                  display: 'block',
                  marginTop: 'var(--space-1)',
                }}
              >
                Controller is not currently paired with an aircraft — flight linking unavailable.
              </small>
            )}
          </div>

          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Created By * (3-letter code)</label>
              <input
                style={{ ...inputStyle, ...monoStyle, textTransform: 'uppercase' }}
                value={createdBy}
                onChange={(e) => setCreatedBy(e.target.value.toUpperCase().slice(0, 3))}
                maxLength={3}
                placeholder="e.g. LAR"
                required
              />
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Sign Off * (3-letter code)</label>
              <input
                style={{ ...inputStyle, ...monoStyle, textTransform: 'uppercase' }}
                value={signOff}
                onChange={(e) => setSignOff(e.target.value.toUpperCase().slice(0, 3))}
                maxLength={3}
                placeholder="e.g. JON"
                required
              />
            </div>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Remark * — WHAT was observed / WHEN</label>
            <textarea
              style={{ ...inputStyle, minHeight: '60px', resize: 'vertical' }}
              value={remark}
              onChange={(e) => setRemark(e.target.value)}
              placeholder="Describe what was observed and when..."
              required
            />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Action Taken * — WHAT done / HOW verified / which manual</label>
            <textarea
              style={{ ...inputStyle, minHeight: '60px', resize: 'vertical' }}
              value={actionTaken}
              onChange={(e) => setActionTaken(e.target.value)}
              placeholder="Describe what was done, how it was verified..."
              required
            />
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Category *</label>
              <select
                style={inputStyle}
                value={maintCategory}
                onChange={(e) => setMaintCategory(e.target.value)}
                required
              >
                <option value="">— Select —</option>
                <option value="Scheduled">Scheduled</option>
                <option value="Unscheduled">Unscheduled</option>
              </select>
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Type *</label>
              <select
                style={inputStyle}
                value={maintType}
                onChange={(e) => setMaintType(e.target.value)}
                required
              >
                <option value="">— Select —</option>
                <option value="Repair">Repair</option>
                <option value="Modification">Modification</option>
                <option value="Overhaul">Overhaul</option>
                <option value="Test">Test</option>
              </select>
            </div>
          </div>
          {target.kind === 'aircraft' && modelId !== null && (
            <div style={{ marginBottom: 'var(--space-4)' }}>
              <div style={{
                fontSize: '0.75rem',
                fontWeight: 600,
                textTransform: 'uppercase',
                letterSpacing: '0.08em',
                color: 'var(--color-text-muted)',
                marginBottom: 'var(--space-2)',
              }}>
                Inspection Checklist (EASA OSO #3)
              </div>
              {templateListQuery.isLoading && (
                <p style={{ fontSize: '0.8rem', color: 'var(--color-text-muted)' }}>Loading template…</p>
              )}
              {!templateListQuery.isLoading && !matchedTemplate && (
                <p style={{ fontSize: '0.8rem', color: 'var(--color-text-muted)', fontStyle: 'italic' }}>
                  No inspection template found for this model — record will be saved without a checklist.
                </p>
              )}
              {matchedTemplate && templateDetailQuery.isLoading && (
                <p style={{ fontSize: '0.8rem', color: 'var(--color-text-muted)' }}>Loading checklist…</p>
              )}
              {templateDetail && templateDetail.sections.map((section) => (
                <div key={section.section_order} style={{ marginBottom: 'var(--space-3)' }}>
                  <div style={{
                    fontSize: '0.7rem',
                    fontWeight: 700,
                    textTransform: 'uppercase',
                    letterSpacing: '0.06em',
                    color: 'var(--color-text-secondary)',
                    marginBottom: 'var(--space-2)',
                    paddingBottom: 'var(--space-1)',
                    borderBottom: '1px solid var(--color-border)',
                  }}>
                    {section.section_name}
                  </div>
                  {section.items.map((item: ChecklistItemSummary) => {
                    const status = checklistStatus[item.id] ?? 'pending';
                    const isNok = status === 'nok';
                    return (
                      <div key={item.id} style={{
                        padding: 'var(--space-2) 0',
                        borderBottom: '1px solid var(--color-border)',
                      }}>
                        <div style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'flex-start' }}>
                          <span style={{
                            fontSize: '0.7rem',
                            color: 'var(--color-text-muted)',
                            minWidth: '20px',
                            paddingTop: '2px',
                          }}>
                            {item.item_order}.
                          </span>
                          <div style={{ flex: 1 }}>
                            <div style={{ fontSize: '0.82rem', color: 'var(--color-text-primary)', lineHeight: 1.45 }}>
                              {item.item_text}
                              {item.is_required && (
                                <span style={{ color: 'var(--color-critical)', marginLeft: 'var(--space-1)', fontSize: '0.7rem' }}>*</span>
                              )}
                            </div>
                            {item.reference_manual && (
                              <div style={{ fontSize: '0.7rem', color: 'var(--color-text-muted)', marginTop: '2px' }}>
                                {item.reference_manual}
                              </div>
                            )}
                            <div style={{ display: 'flex', gap: 'var(--space-2)', marginTop: 'var(--space-2)' }}>
                              {(['ok', 'nok', 'na'] as const).map((s) => (
                                <button
                                  key={s}
                                  type="button"
                                  onClick={() => setItemStatus(item.id, s)}
                                  style={{
                                    padding: '2px 10px',
                                    borderRadius: '99px',
                                    fontSize: '0.72rem',
                                    fontWeight: 600,
                                    cursor: 'pointer',
                                    border: status === s ? 'none' : '1px solid var(--color-border)',
                                    background: status === s
                                      ? s === 'ok' ? 'var(--color-ok)'
                                      : s === 'nok' ? 'var(--color-critical)'
                                      : 'var(--color-text-muted)'
                                      : 'var(--color-bg-input)',
                                    color: status === s ? '#fff' : 'var(--color-text-muted)',
                                  }}
                                >
                                  {s.toUpperCase()}
                                </button>
                              ))}
                            </div>
                            {isNok && (
                              <textarea
                                style={{
                                  ...inputStyle,
                                  marginTop: 'var(--space-2)',
                                  minHeight: '48px',
                                  fontSize: '0.8rem',
                                  resize: 'vertical',
                                }}
                                placeholder="Describe the fault (required for NOK) *"
                                value={checklistNotes[item.id] ?? ''}
                                onChange={(e) =>
                                  setChecklistNotes((prev) => ({ ...prev, [item.id]: e.target.value }))
                                }
                              />
                            )}
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
          )}
          {error && (
            <p
              style={{
                color: 'var(--color-critical)',
                fontSize: '0.8rem',
                marginBottom: 'var(--space-3)',
              }}
            >
              {error}
            </p>
          )}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>
              Cancel
            </button>
            <button
              type="submit"
              disabled={mutation.isPending}
              style={submitBtnStyle(mutation.isPending)}
            >
              {mutation.isPending ? 'Saving...' : target.kind === 'controller' ? 'Add Event' : 'Add Record'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
