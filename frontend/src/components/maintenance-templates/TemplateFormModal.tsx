// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { z } from 'zod';
import { zodResolver } from '@hookform/resolvers/zod';
import {
  useCreateScheduleTemplate,
  useScheduleTemplates,
  useUpdateScheduleTemplate,
} from '../../hooks/useMaintenanceScheduleTemplates';
import { showToast } from '../../store/toastStore';
import type { AircraftModelWithCount } from '../../api/aircraftModels';
import type {
  CreateTemplateInput,
  IntervalType,
  MaintenanceScheduleTemplate,
  SecondaryIntervalType,
  UpdateTemplateInput,
} from '../../types/maintenanceScheduleTemplate';
import styles from '../maintenance/MaintenanceModal.module.css';

const INTERVAL_TYPES: IntervalType[] = ['flight_hours', 'calendar_days', 'cycles'];
const SECONDARY_TYPES: SecondaryIntervalType[] = ['flight_hours', 'calendar_days'];

const schema = z
  .object({
    model_id: z.coerce.number().int().positive('Pick a model'),
    name: z.string().trim().min(1, 'Name is required').max(100, 'Max 100 characters'),
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
  models: AircraftModelWithCount[];
  /** Pre-select this model id when creating (e.g. user clicked "Add" inside a model group). */
  defaultModelId?: number;
  /** Pass to enter edit mode. model_id selector is disabled in this mode. */
  existingTemplate?: MaintenanceScheduleTemplate;
}

function toFormValues(
  existing: MaintenanceScheduleTemplate | undefined,
  defaultModelId: number | undefined,
  models: AircraftModelWithCount[],
): FormValues {
  const fallbackModelId = defaultModelId ?? models[0]?.id ?? 0;
  return {
    model_id: existing?.model_id ?? fallbackModelId,
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

/// Pre-fills form values from a source template the user wants to clone.
/// Differences from toFormValues:
///   - model_id keeps the user's currently-selected model (don't force-flip
///     to the source's model — they may be cloning to a different model).
///   - name gets " (copy)" suffix so the user must rename before save.
function toCloneValues(
  source: MaintenanceScheduleTemplate,
  currentModelId: number,
): FormValues {
  return {
    model_id: currentModelId || source.model_id,
    name: `${source.name} (copy)`,
    description: source.description ?? '',
    interval_type: source.interval_type,
    interval_value: source.interval_value,
    warning_threshold: source.warning_threshold,
    is_critical: source.is_critical,
    has_secondary:
      source.secondary_interval_type !== null &&
      source.secondary_interval_type !== undefined,
    secondary_interval_type: source.secondary_interval_type ?? undefined,
    secondary_interval_value: source.secondary_interval_value ?? undefined,
  };
}

export function TemplateFormModal({
  isOpen,
  onClose,
  models,
  defaultModelId,
  existingTemplate,
}: Props) {
  const create = useCreateScheduleTemplate();
  const update = useUpdateScheduleTemplate();

  const {
    register,
    handleSubmit,
    reset,
    watch,
    getValues,
    formState: { errors, isSubmitting },
  } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: toFormValues(existingTemplate, defaultModelId, models),
  });

  // Clone-from-existing only matters in create mode. Fetch every active
  // template across all models — the data is small and per-org, so always
  // fetch and hide the picker in edit mode below.
  const isCreating = !existingTemplate;
  const { data: cloneCandidates } = useScheduleTemplates();
  const [cloneSourceId, setCloneSourceId] = useState<number | ''>('');

  const resetCreate = create.reset;
  const resetUpdate = update.reset;
  useEffect(() => {
    if (isOpen) {
      reset(toFormValues(existingTemplate, defaultModelId, models));
      setCloneSourceId('');
      resetCreate();
      resetUpdate();
    }
  }, [isOpen, existingTemplate, defaultModelId, models, reset, resetCreate, resetUpdate]);

  const onCloneSourceChange = (value: string) => {
    if (!value) {
      setCloneSourceId('');
      reset(toFormValues(undefined, defaultModelId, models));
      return;
    }
    const id = Number(value);
    setCloneSourceId(id);
    const source = (cloneCandidates ?? []).find((c) => c.id === id);
    if (source) {
      reset(toCloneValues(source, getValues('model_id')));
    }
  };

  const hasSecondary = watch('has_secondary');
  const mutation = existingTemplate ? update : create;

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
    };

    if (existingTemplate) {
      const payload: UpdateTemplateInput = base;
      update.mutate(
        { id: existingTemplate.id, input: payload },
        {
          onSuccess: () => {
            showToast(`Template updated · ${base.name}`, 'success');
            onClose();
          },
        },
      );
    } else {
      const payload: CreateTemplateInput = { ...base, model_id: values.model_id };
      create.mutate(payload, {
        onSuccess: () => {
          showToast(`Template created · ${base.name}`, 'success');
          onClose();
        },
      });
    }
  });

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>
          {existingTemplate ? 'Edit template' : 'Add template'}
        </h2>

        {mutation.isError && (
          <div className={styles.errorBanner}>
            {(mutation.error as Error).message ?? 'Failed to save template'}
          </div>
        )}

        <form onSubmit={onSubmit}>
          {isCreating && (cloneCandidates?.length ?? 0) > 0 && (
            <div className={styles.field}>
              <label className={styles.label}>Clone from (optional)</label>
              <select
                className={styles.select}
                value={cloneSourceId}
                onChange={(e) => onCloneSourceChange(e.target.value)}
              >
                <option value="">(blank)</option>
                {(cloneCandidates ?? []).map((c) => {
                  const modelName = models.find((m) => m.id === c.model_id)?.name
                    ?? `Model #${c.model_id}`;
                  return (
                    <option key={c.id} value={c.id}>
                      {c.name} ({modelName})
                    </option>
                  );
                })}
              </select>
              <span className={styles.hint}>
                Pre-fills all fields from the chosen template. The name will be
                suffixed with " (copy)" — rename before saving.
              </span>
            </div>
          )}

          <div className={styles.field}>
            <label className={styles.label}>Aircraft model *</label>
            <select
              className={`${styles.select} ${errors.model_id ? styles.inputError : ''}`}
              disabled={!!existingTemplate}
              {...register('model_id')}
            >
              {models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                  {m.manufacturer ? ` (${m.manufacturer})` : ''}
                </option>
              ))}
            </select>
            {errors.model_id && (
              <span className={styles.errorText}>{errors.model_id.message}</span>
            )}
          </div>

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
                      <option key={t} value={t}>
                        {t.replace('_', ' ')}
                      </option>
                    ))}
                  </select>
                  <span className={styles.hint}>
                    Cycles cannot be used as a secondary trigger.
                  </span>
                  {errors.secondary_interval_type && (
                    <span className={styles.errorText}>
                      {errors.secondary_interval_type.message}
                    </span>
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
                    <span className={styles.errorText}>
                      {errors.secondary_interval_value.message}
                    </span>
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
              {mutation.isPending
                ? 'Saving…'
                : existingTemplate
                  ? 'Save changes'
                  : 'Create'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
