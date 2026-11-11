// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { zodResolver } from '@hookform/resolvers/zod';
import {
  useCreateMaintenanceSchedule,
  useUpdateMaintenanceSchedule,
} from '../../hooks/useMaintenanceSchedules';
import { showToast } from '../../store/toastStore';
import { fetchInstructionTemplates } from '../../api/maintenanceInstructionTemplates';
import type {
  CreateSchedulePayload,
  IntervalType,
  MaintenanceSchedule,
  SecondaryIntervalType,
  UpdateSchedulePayload,
} from '../../types/maintenanceSchedule';
import type { TemplateListItem } from '../../types/maintenanceInstructionTemplate';
import { CreateTemplateModal } from './CreateTemplateModal';
import styles from './MaintenanceModal.module.css';

const INTERVAL_TYPES: IntervalType[] = ['flight_hours', 'calendar_days', 'cycles'];
const SECONDARY_TYPES: SecondaryIntervalType[] = ['flight_hours', 'calendar_days'];

const schema = z
  .object({
    name: z.string().trim().min(3, 'Name must be at least 3 characters'),
    description: z.string().optional(),
    interval_type: z.enum(['flight_hours', 'calendar_days', 'cycles']),
    interval_value: z.coerce.number().positive('Must be greater than 0'),
    warning_threshold: z.coerce
      .number()
      .min(0.5, 'Must be at least 0.5')
      .max(0.99, 'Must be at most 0.99'),
    is_critical: z.boolean(),
    has_secondary: z.boolean(),
    secondary_interval_type: z.enum(['flight_hours', 'calendar_days']).optional(),
    secondary_interval_value: z.coerce.number().positive().optional(),
  })
  .superRefine((data, ctx) => {
    if (!data.has_secondary) return;
    if (!data.secondary_interval_type) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['secondary_interval_type'],
        message: 'Select a secondary interval type',
      });
    }
    if (data.secondary_interval_value === undefined || data.secondary_interval_value === null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['secondary_interval_value'],
        message: 'Value is required when secondary trigger is enabled',
      });
    }
  });

type FormValues = z.infer<typeof schema>;

interface Props {
  isOpen: boolean;
  onClose: () => void;
  aircraftId: number;
  aircraftModelId: number | null;
  canCreateTemplate: boolean;
  existingSchedule?: MaintenanceSchedule;
}

function toFormValues(existing?: MaintenanceSchedule): FormValues {
  return {
    name: existing?.name ?? '',
    description: existing?.description ?? '',
    interval_type: existing?.interval_type ?? 'flight_hours',
    interval_value: existing?.interval_value ?? 50,
    warning_threshold: existing?.warning_threshold ?? 0.8,
    is_critical: existing?.is_critical ?? false,
    has_secondary:
      existing?.secondary_interval_type !== null &&
      existing?.secondary_interval_type !== undefined,
    secondary_interval_type: existing?.secondary_interval_type ?? undefined,
    secondary_interval_value: existing?.secondary_interval_value ?? undefined,
  };
}

export function ScheduleFormModal({
  isOpen,
  onClose,
  aircraftId,
  aircraftModelId,
  canCreateTemplate,
  existingSchedule,
}: Props) {
  const create = useCreateMaintenanceSchedule();
  const update = useUpdateMaintenanceSchedule();
  const queryClient = useQueryClient();

  const {
    register,
    handleSubmit,
    reset,
    watch,
    formState: { errors, isSubmitting },
  } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: toFormValues(existingSchedule),
  });

  const [templateId, setTemplateId] = useState<number | null>(
    existingSchedule?.instruction_template_id ?? null,
  );
  const [showCreateTemplate, setShowCreateTemplate] = useState(false);

  // Only re-apply defaults when the modal transitions open, or the record it
  // targets changes. Mutation objects are excluded deliberately — their identity
  // changes every render and would otherwise re-reset the form on every keystroke.
  const resetCreate = create.reset;
  const resetUpdate = update.reset;
  useEffect(() => {
    if (isOpen) {
      reset(toFormValues(existingSchedule));
      setTemplateId(existingSchedule?.instruction_template_id ?? null);
      resetCreate();
      resetUpdate();
    }
  }, [isOpen, existingSchedule, reset, resetCreate, resetUpdate]);

  // Fetch active templates and filter client-side to generic templates plus
  // those matching this aircraft's model.
  const templatesQuery = useQuery({
    queryKey: ['instruction-templates', 'picker'],
    queryFn: () => fetchInstructionTemplates(),
    enabled: isOpen,
    staleTime: 60_000,
  });
  const availableTemplates = (templatesQuery.data ?? []).filter(
    (t: TemplateListItem) =>
      t.aircraft_model_id === null || t.aircraft_model_id === aircraftModelId,
  );

  const hasSecondary = watch('has_secondary');
  const mutation = existingSchedule ? update : create;

  if (!isOpen) return null;

  const onSubmit = handleSubmit((values) => {
    const trimmedDesc = values.description?.trim();
    const base = {
      name: values.name.trim(),
      description: trimmedDesc ? trimmedDesc : null,
      interval_type: values.interval_type,
      interval_value: values.interval_value,
      warning_threshold: values.warning_threshold,
      is_critical: values.is_critical,
      secondary_interval_type: values.has_secondary
        ? values.secondary_interval_type ?? null
        : null,
      secondary_interval_value: values.has_secondary
        ? values.secondary_interval_value ?? null
        : null,
      instruction_template_id: templateId,
    };

    if (existingSchedule) {
      const payload: UpdateSchedulePayload = base;
      update.mutate(
        { id: existingSchedule.id, payload },
        {
          onSuccess: () => {
            showToast(`Schedule updated · ${base.name}`, 'success');
            onClose();
          },
        },
      );
    } else {
      const payload: CreateSchedulePayload = { ...base, aircraft_id: aircraftId };
      create.mutate(payload, {
        onSuccess: () => {
          showToast(`Schedule created · ${base.name}`, 'success');
          onClose();
        },
      });
    }
  });

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>
          {existingSchedule ? 'Edit schedule' : 'Add schedule'}
        </h2>

        {mutation.isError && (
          <div className={styles.errorBanner}>
            {(mutation.error as Error).message ?? 'Failed to save schedule'}
          </div>
        )}

        <form onSubmit={onSubmit}>
          <div className={styles.field}>
            <label className={styles.label}>Name *</label>
            <input
              className={`${styles.input} ${errors.name ? styles.inputError : ''}`}
              placeholder="e.g. Propeller inspection"
              {...register('name')}
            />
            {errors.name && <span className={styles.errorText}>{errors.name.message}</span>}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Description</label>
            <input
              className={styles.input}
              placeholder="Optional notes or manual reference"
              {...register('description')}
            />
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Inspection checklist</label>
            <select
              className={styles.select}
              value={String(templateId ?? '')}
              onChange={(e) =>
                setTemplateId(e.target.value ? Number(e.target.value) : null)
              }
            >
              {existingSchedule?.instruction_template_id == null && (
                <option value="">— No checklist —</option>
              )}
              {templateId !== null &&
                !availableTemplates.some((t) => t.id === templateId) && (
                  <option value={templateId}>Currently linked (#{templateId})</option>
                )}
              {availableTemplates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                  {t.source === 'manufacturer' ? ' (manufacturer)' : ''}
                </option>
              ))}
            </select>
            {canCreateTemplate && (
              <button
                type="button"
                className={styles.btnSaveDraft}
                onClick={() => setShowCreateTemplate(true)}
              >
                + New checklist
              </button>
            )}
            {availableTemplates.length === 0 && (
              <span className={styles.hint}>
                {canCreateTemplate
                  ? 'No inspection checklists for this aircraft model yet. Create one with "+ New checklist" above.'
                  : "No inspection checklists for this aircraft model yet. An admin can add one from the aircraft's Model & checklists tab."}
              </span>
            )}
          </div>

          <div className={styles.section}>
            <div className={styles.sectionTitle}>Primary trigger</div>
            <div className={styles.row}>
              <div className={styles.field}>
                <label className={styles.label}>Type *</label>
                <select className={styles.select} {...register('interval_type')}>
                  {INTERVAL_TYPES.map((t) => (
                    <option key={t} value={t}>
                      {t.replace('_', ' ')}
                    </option>
                  ))}
                </select>
              </div>
              <div className={styles.field}>
                <label className={styles.label}>Interval *</label>
                <input
                  type="number"
                  min={0}
                  step="any"
                  className={`${styles.input} ${errors.interval_value ? styles.inputError : ''}`}
                  {...register('interval_value')}
                />
                {errors.interval_value && (
                  <span className={styles.errorText}>{errors.interval_value.message}</span>
                )}
              </div>
            </div>
            <div className={styles.row}>
              <div className={styles.field}>
                <label className={styles.label}>Warning at (% consumed)</label>
                <input
                  type="number"
                  step="0.01"
                  min={0.5}
                  max={0.99}
                  className={`${styles.input} ${errors.warning_threshold ? styles.inputError : ''}`}
                  {...register('warning_threshold')}
                />
                <span className={styles.hint}>Between 0.5 and 0.99 (default 0.8 = 80%)</span>
                {errors.warning_threshold && (
                  <span className={styles.errorText}>{errors.warning_threshold.message}</span>
                )}
              </div>
              <div className={styles.field}>
                <label className={styles.checkbox}>
                  <input type="checkbox" {...register('is_critical')} />
                  Critical (flagged in alerts)
                </label>
              </div>
            </div>
          </div>

          <div className={styles.section}>
            <label className={styles.checkbox}>
              <input type="checkbox" {...register('has_secondary')} />
              Add a second trigger (whichever comes first)
            </label>
            {hasSecondary && (
              <div className={styles.row}>
                <div className={styles.field}>
                  <label className={styles.label}>Secondary type *</label>
                  <select className={styles.select} {...register('secondary_interval_type')}>
                    <option value="">— Select —</option>
                    {SECONDARY_TYPES.map((t) => (
                      <option key={t} value={t}>{t.replace('_', ' ')}</option>
                    ))}
                    <option value="cycles" disabled>cycles (not supported as secondary)</option>
                  </select>
                  <span className={styles.hint}>Cycles cannot be used as a secondary trigger.</span>
                  {errors.secondary_interval_type && (
                    <span className={styles.errorText}>{errors.secondary_interval_type.message}</span>
                  )}
                </div>
                <div className={styles.field}>
                  <label className={styles.label}>Secondary interval *</label>
                  <input
                    type="number"
                    min={0}
                    step="any"
                    className={`${styles.input} ${errors.secondary_interval_value ? styles.inputError : ''}`}
                    {...register('secondary_interval_value')}
                  />
                  {errors.secondary_interval_value && (
                    <span className={styles.errorText}>{errors.secondary_interval_value.message}</span>
                  )}
                </div>
              </div>
            )}
          </div>

          <div className={styles.actions}>
            <button type="button" className={styles.btnCancel} onClick={onClose}>
              Cancel
            </button>
            <button
              type="submit"
              className={styles.btnPrimary}
              disabled={isSubmitting || mutation.isPending}
            >
              {mutation.isPending ? 'Saving…' : existingSchedule ? 'Save changes' : 'Create'}
            </button>
          </div>
        </form>

        {showCreateTemplate && (
          <CreateTemplateModal
            defaultModelId={aircraftModelId}
            onClose={() => setShowCreateTemplate(false)}
            onSuccess={(newId) => {
              setShowCreateTemplate(false);
              void queryClient.invalidateQueries({ queryKey: ['instruction-templates'] });
              setTemplateId(newId);
            }}
          />
        )}
      </div>
    </div>
  );
}
