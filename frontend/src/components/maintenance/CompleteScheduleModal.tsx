// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useCallback, useEffect, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { Controller, useForm } from 'react-hook-form';
import { z } from 'zod';
import { zodResolver } from '@hookform/resolvers/zod';
import { useCurrentUser } from '../../hooks/useCurrentUser';
import { useCompleteMaintenanceSchedule } from '../../hooks/useMaintenanceSchedules';
import { showToast } from '../../store/toastStore';
import { fetchInstructionTemplateDetail } from '../../api/maintenanceInstructionTemplates';
import { saveMaintenanceCompletions } from '../../api/maintenance';
import { deleteDraft, fetchDraft, upsertDraft } from '../../api/maintenanceDrafts';
import type {
  MaintenanceType,
  ScheduleWithStatus,
} from '../../types/maintenanceSchedule';
import type { TemplateDetail, ChecklistItemSummary } from '../../types/maintenanceInstructionTemplate';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import { ScheduleStatusBadge } from './ScheduleStatusBadge';
import { IsoDateInput } from '../inputs/IsoDateInput';
import styles from './MaintenanceModal.module.css';

const MAINT_TYPES: MaintenanceType[] = ['Repair', 'Modification', 'Overhaul', 'Test'];

const schema = z
  .object({
    performed_date: z.string().min(1, 'Date is required'),
    maint_type: z.enum(['Repair', 'Modification', 'Overhaul', 'Test']),
    remark: z.string().trim().min(5, 'Must be at least 5 characters (EASA OSO #3)'),
    action_taken: z.string().trim().min(5, 'Must be at least 5 characters (EASA OSO #3)'),
    sign_off: z
      .string()
      .regex(/^[A-Z]{3}$/, 'Must be exactly 3 uppercase letters'),
  })
  .refine(
    (data) => {
      const picked = new Date(data.performed_date);
      const today = new Date();
      today.setHours(23, 59, 59, 999);
      return picked.getTime() <= today.getTime();
    },
    { path: ['performed_date'], message: 'Performed date cannot be in the future' },
  )
  .refine(
    (data) => {
      const picked = new Date(data.performed_date);
      const oneYearAgo = new Date();
      oneYearAgo.setFullYear(oneYearAgo.getFullYear() - 1);
      return picked.getTime() >= oneYearAgo.getTime();
    },
    {
      path: ['performed_date'],
      message: 'Performed date cannot be more than 1 year in the past',
    },
  );

type FormValues = z.infer<typeof schema>;

interface Props {
  isOpen: boolean;
  onClose: () => void;
  schedule: ScheduleWithStatus;
}

function todayIso(): string {
  return new Date().toISOString().slice(0, 10);
}

function daysBack(value: string): number {
  if (!value) return 0;
  const d = new Date(value);
  const diffMs = Date.now() - d.getTime();
  return Math.floor(diffMs / (1000 * 60 * 60 * 24));
}

export function CompleteScheduleModal({ isOpen, onClose, schedule }: Props) {
  const { data: user } = useCurrentUser();
  const complete = useCompleteMaintenanceSchedule();

  const {
    register,
    handleSubmit,
    control,
    reset,
    watch,
    formState: { errors, isSubmitting },
  } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      performed_date: todayIso(),
      maint_type: 'Test',
      remark: '',
      action_taken: '',
      sign_off: '',
    },
  });

  // Exclude the whole `complete` mutation object from the dep array — its
  // identity changes every render and would re-run this effect, wiping typed
  // input between keystrokes. Only depend on the stable reset callback.
  const resetComplete = complete.reset;
  useEffect(() => {
    if (isOpen) {
      reset({
        performed_date: todayIso(),
        maint_type: 'Test',
        remark: '',
        action_taken: '',
        sign_off: user?.user_code ?? '',
      });
      resetComplete();
    }
  }, [isOpen, user?.user_code, reset, resetComplete]);

  const templateId = schedule.instruction_template_id ?? null;

  const [checklistStatus, setChecklistStatus] = useState<Record<number, 'ok' | 'nok' | 'na' | 'pending'>>({});
  const [checklistNotes, setChecklistNotes] = useState<Record<number, string>>({});

  // Load the template linked to THIS schedule (not a guess from the model's
  // template list). NULL = no checklist linked; completion is still allowed.
  const templateDetailQuery = useQuery({
    queryKey: ['instruction-template-detail', templateId, isOpen],
    queryFn: () => fetchInstructionTemplateDetail(templateId!),
    enabled: isOpen && templateId !== null,
    staleTime: 60_000,
  });
  const templateDetail: TemplateDetail | null = templateDetailQuery.data ?? null;

  // Draft loading — 404 means no draft exists, not a retryable error.
  const draftQuery = useQuery({
    queryKey: ['maintenance-draft', schedule.id],
    queryFn: () => fetchDraft(schedule.id),
    enabled: isOpen,
    retry: false,
    staleTime: 0,
  });
  const existingDraft = draftQuery.isSuccess ? draftQuery.data : null;

  const saveDraftMutation = useMutation({
    mutationFn: (payload: Parameters<typeof upsertDraft>[1]) =>
      upsertDraft(schedule.id, payload),
    onSuccess: () =>
      showToast('Draft saved — you can resume this inspection later.', 'success'),
    onError: () => showToast('Failed to save draft.', 'error'),
  });

  // Initialise all items to 'pending' when template detail arrives.
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

  // Reset checklist state when modal opens or closes.
  useEffect(() => {
    if (!isOpen) {
      setChecklistStatus({});
      setChecklistNotes({});
    }
  }, [isOpen]);

  // Pre-fill from draft once it arrives. Runs after the initial reset() in
  // the open-effect above, so the draft values overwrite the defaults.
  useEffect(() => {
    if (!isOpen || !existingDraft) return;
    reset({
      performed_date: existingDraft.form_data.performed_date || todayIso(),
      maint_type:
        (existingDraft.form_data.maint_type as FormValues['maint_type']) ||
        'Test',
      remark: existingDraft.form_data.remark || '',
      action_taken: existingDraft.form_data.action_taken || '',
      sign_off: existingDraft.form_data.sign_off || user?.user_code || '',
    });
    const statusFromDraft = existingDraft.checklist_status as Record<
      string,
      'ok' | 'nok' | 'na' | 'pending'
    >;
    const notesFromDraft = existingDraft.checklist_notes as Record<string, string>;
    setChecklistStatus((prev) => {
      const next = { ...prev };
      for (const [k, v] of Object.entries(statusFromDraft)) {
        next[Number(k)] = v;
      }
      return next;
    });
    setChecklistNotes((prev) => ({
      ...prev,
      ...Object.fromEntries(
        Object.entries(notesFromDraft).map(([k, v]) => [Number(k), v]),
      ),
    }));
  }, [existingDraft, isOpen, reset, user?.user_code]);

  const setItemStatus = useCallback(
    (itemId: number, status: 'ok' | 'nok' | 'na') => {
      setChecklistStatus((prev) => ({ ...prev, [itemId]: status }));
    },
    [],
  );

  if (!isOpen) return null;

  const handleSaveDraft = () => {
    const values = watch();
    saveDraftMutation.mutate({
      saved_by: values.sign_off || user?.user_code || '???',
      form_data: {
        performed_date: values.performed_date,
        maint_type: values.maint_type,
        remark: values.remark,
        action_taken: values.action_taken,
        sign_off: values.sign_off,
      },
      checklist_status: Object.fromEntries(
        Object.entries(checklistStatus).map(([k, v]) => [k, v]),
      ) as Record<string, 'ok' | 'nok' | 'na' | 'pending'>,
      checklist_notes: Object.fromEntries(
        Object.entries(checklistNotes).map(([k, v]) => [String(k), v]),
      ),
    });
  };

  const onSubmit = handleSubmit(async (values) => {
    // Validate checklist before submitting.
    if (templateDetail) {
      for (const section of templateDetail.sections) {
        for (const item of section.items) {
          if (item.is_required && (!checklistStatus[item.id] || checklistStatus[item.id] === 'pending')) {
            // RHF doesn't manage checklist state — set a manual error via setError or show toast.
            // Use showToast for simplicity (already imported).
            showToast(
              `"${section.section_name}" item ${item.item_order}: required — mark OK, NOK, or N/A.`,
              'error',
            );
            return;
          }
          if (checklistStatus[item.id] === 'nok' && !checklistNotes[item.id]?.trim()) {
            showToast(
              `"${section.section_name}" item ${item.item_order}: NOK requires a note.`,
              'error',
            );
            return;
          }
        }
      }
    }

    complete.mutate(
      {
        id: schedule.id,
        payload: {
          performed_date: values.performed_date,
          action_taken: values.action_taken.trim(),
          remark: values.remark.trim(),
          maint_type: values.maint_type,
          sign_off: values.sign_off.toUpperCase(),
        },
      },
      {
        onSuccess: async (data) => {
          // Save checklist completions if a template was linked.
          if (templateDetail && templateId !== null) {
            const completions = templateDetail.sections.flatMap((section) =>
              section.items.map((item: ChecklistItemSummary) => ({
                checklist_item_id: item.id,
                status: checklistStatus[item.id] ?? 'pending',
                note: checklistNotes[item.id]?.trim() || undefined,
                completed_by: values.sign_off.toUpperCase(),
              })),
            );
            try {
              const completionResult = await saveMaintenanceCompletions(data.maintenance_id, {
                template_id: templateId,
                completions,
              });
              if (completionResult.defects_created > 0) {
                showToast(
                  `${completionResult.defects_created} defect${completionResult.defects_created > 1 ? 's' : ''} reported automatically from NOK items.`,
                  'info',
                );
              }
            } catch (e) {
              console.error('Failed to save checklist completions:', e);
            }
          }
          // Clean up draft if one existed — non-fatal on failure.
          void deleteDraft(schedule.id).catch(() => { /* non-fatal */ });
          showToast(`Maintenance logged · ${schedule.name}`, 'success');
          onClose();
        },
      },
    );
  });

  const performedDate = watch('performed_date');
  const backDays = daysBack(performedDate);

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>Complete: {schedule.name}</h2>
        <div className={styles.contextStrip}>
          <strong>
            {formatAircraftLabel({
              equipment_id: schedule.aircraft_equipment_id,
              model: schedule.aircraft_model,
              serial_number: schedule.aircraft_serial_number,
            })}
          </strong>
          <span>·</span>
          <ScheduleStatusBadge status={schedule.status} />
          <span>·</span>
          <span>{Math.round(schedule.consumption_pct)}% consumed</span>
        </div>

        {existingDraft && (
          <div className={styles.draftBanner}>
            <span>
              Draft saved {existingDraft.updated_at.slice(0, 10)} by{' '}
              {existingDraft.saved_by} — form pre-filled.
            </span>
          </div>
        )}

        {complete.isError && (
          <div className={styles.errorBanner}>
            {(complete.error as Error).message ?? 'Failed to log maintenance'}
          </div>
        )}

        <form onSubmit={onSubmit}>
          <div className={styles.row}>
            <div className={styles.field}>
              <label className={styles.label}>Performed date *</label>
              <Controller
                name="performed_date"
                control={control}
                render={({ field }) => (
                  <IsoDateInput
                    className={`${styles.input} ${errors.performed_date ? styles.inputError : ''}`}
                    value={field.value ?? ''}
                    onChange={(e) => field.onChange(e.target.value)}
                    onBlur={field.onBlur}
                    name={field.name}
                    max={todayIso()}
                  />
                )}
              />
              {errors.performed_date && (
                <span className={styles.errorText}>{errors.performed_date.message}</span>
              )}
              {!errors.performed_date && backDays > 30 && (
                <span className={styles.hint}>
                  Retroactive log: {backDays} days ago
                </span>
              )}
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Type *</label>
              <select className={styles.select} {...register('maint_type')}>
                {MAINT_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Remark * — WHAT was observed / WHEN</label>
            <textarea
              id="complete-remark"
              className={`${styles.textarea} ${errors.remark ? styles.inputError : ''}`}
              placeholder="Scheduled 50h propeller inspection on UAV-001, no anomalies found"
              {...register('remark')}
            />
            {errors.remark && (
              <span className={styles.errorText}>{errors.remark.message}</span>
            )}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>
              Action taken * — WHAT was done / HOW verified / which manual
            </label>
            <textarea
              id="complete-action-taken"
              className={`${styles.textarea} ${errors.action_taken ? styles.inputError : ''}`}
              placeholder="Inspected per DJI Air 2s user manual section 4.2, all blades within tolerance"
              {...register('action_taken')}
            />
            {errors.action_taken && (
              <span className={styles.errorText}>{errors.action_taken.message}</span>
            )}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Sign-off * (3-letter code)</label>
            <input
              id="complete-sign-off"
              className={`${styles.input} ${styles.inputMono} ${
                errors.sign_off ? styles.inputError : ''
              }`}
              maxLength={3}
              {...register('sign_off', {
                setValueAs: (v: string) => (v ?? '').toUpperCase().slice(0, 3),
              })}
            />
            {errors.sign_off && (
              <span className={styles.errorText}>{errors.sign_off.message}</span>
            )}
          </div>

          <div className={styles.checklistSection}>
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
            {templateId === null && (
              <p className={styles.hint} style={{ fontStyle: 'italic' }}>
                No inspection checklist linked to this schedule. You can still
                log completion; link a checklist via Edit schedule on the
                Maintenance Schedules page.
              </p>
            )}
            {templateId !== null && templateDetailQuery.isLoading && (
              <p className={styles.hint}>Loading checklist…</p>
            )}
            {templateDetail && templateDetail.sections.map((section) => (
                <div key={section.section_order} style={{ marginBottom: 'var(--space-3)' }}>
                  <div style={{
                    fontSize: '0.7rem', fontWeight: 700, textTransform: 'uppercase',
                    letterSpacing: '0.06em', color: 'var(--color-text-secondary)',
                    marginBottom: 'var(--space-2)', paddingBottom: 'var(--space-1)',
                    borderBottom: '1px solid var(--color-border)',
                  }}>
                    {section.section_name}
                  </div>
                  {section.items.map((item: ChecklistItemSummary) => {
                    const status = checklistStatus[item.id] ?? 'pending';
                    return (
                      <div key={item.id} style={{ padding: 'var(--space-2) 0', borderBottom: '1px solid var(--color-border)' }}>
                        <div style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'flex-start' }}>
                          <span style={{ fontSize: '0.7rem', color: 'var(--color-text-muted)', minWidth: '20px', paddingTop: '2px' }}>
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
                                    padding: '2px 10px', borderRadius: '99px',
                                    fontSize: '0.72rem', fontWeight: 600, cursor: 'pointer',
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
                            {status === 'nok' && (
                              <textarea
                                style={{
                                  marginTop: 'var(--space-2)', minHeight: '48px',
                                  fontSize: '0.8rem', resize: 'vertical', width: '100%',
                                  background: 'var(--color-bg-input)',
                                  border: '1px solid var(--color-border)',
                                  borderRadius: 'var(--radius-sm)',
                                  padding: 'var(--space-2) var(--space-3)',
                                  color: 'var(--color-text-primary)',
                                  fontFamily: 'var(--font-body)',
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

          <p className={styles.easaNotice}>
            This action creates a permanent maintenance record per EASA UAS.SPEC 4.2.3
            (retained ≥ 3 years).
          </p>

          <div className={styles.actions}>
            <button type="button" className={styles.btnCancel} onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className={styles.btnSaveDraft}
              onClick={handleSaveDraft}
              disabled={saveDraftMutation.isPending}
            >
              {saveDraftMutation.isPending ? 'Saving…' : 'Save Draft'}
            </button>
            <button
              type="submit"
              className={styles.btnPrimary}
              disabled={isSubmitting || complete.isPending}
            >
              {complete.isPending ? 'Logging…' : 'Log Completion'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
