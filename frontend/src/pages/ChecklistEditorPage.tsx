// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useMemo, useState } from 'react';
import {
  Controller,
  useFieldArray,
  useForm,
  type Control,
  type FieldErrors,
  type UseFormRegister,
} from 'react-hook-form';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  useNavigate,
  useParams,
  useSearchParams,
} from 'react-router-dom';
import {
  ArrowLeft,
  ChevronDown,
  ChevronUp,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  createModelChecklist,
  updateChecklist,
  type ChecklistFull,
  type ChecklistInputBody,
  type ChecklistType,
  type Responsibility,
} from '../api/checklists';
import { useChecklist } from '../hooks/useFleetStats';
import { IsoDateInput } from '../components/inputs/IsoDateInput';
import { DEFAULT_SECTIONS } from '../utils/checklistDefaults';
import styles from './ChecklistEditorPage.module.css';

const RESPONSIBILITIES: Responsibility[] = ['PF', 'PM', 'PF/PM', 'N/A'];
const TYPES: ChecklistType[] = ['normal_ops', 'qrh'];

// react-hook-form form values mirror the input body shape minus sort_order
// (which is assigned at save time from array position).
interface FormItem {
  label: string;
  value: string;
  responsibility: Responsibility;
}
interface FormSection {
  title: string;
  start_on_new_page: boolean;
  items: FormItem[];
}
interface FormValues {
  type: ChecklistType;
  title: string;
  revision: string;
  revision_date: string;
  sections: FormSection[];
}

function emptyForm(seedSections: FormSection[] = []): FormValues {
  return {
    type: 'normal_ops',
    title: '',
    revision: '01',
    revision_date: '',
    sections: seedSections,
  };
}

function defaultsForm(): FormValues {
  return emptyForm(
    DEFAULT_SECTIONS.map((s) => ({
      title: s.title,
      start_on_new_page: s.start_on_new_page,
      items: [],
    })),
  );
}

function checklistToForm(c: ChecklistFull): FormValues {
  return {
    type: c.type,
    title: c.title,
    revision: c.revision,
    revision_date: c.revision_date ?? '',
    sections: c.sections.map((s) => ({
      title: s.title,
      start_on_new_page: s.start_on_new_page,
      items: s.items.map((i) => ({
        label: i.label,
        value: i.value,
        responsibility: i.responsibility,
      })),
    })),
  };
}

function checklistToCopyForm(source: ChecklistFull): FormValues {
  // Copy structure but blank the identity fields so the operator picks them.
  return {
    type: 'normal_ops',
    title: '',
    revision: '01',
    revision_date: '',
    sections: source.sections.map((s) => ({
      title: s.title,
      start_on_new_page: s.start_on_new_page,
      items: s.items.map((i) => ({
        label: i.label,
        value: i.value,
        responsibility: i.responsibility,
      })),
    })),
  };
}

export function ChecklistEditorPage() {
  const params = useParams<{ modelId: string; checklistId?: string }>();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const modelId = params.modelId ? Number(params.modelId) : null;
  const editingId = params.checklistId ? Number(params.checklistId) : null;
  const isEdit = editingId !== null;
  const copySourceId = searchParams.get('copy')
    ? Number(searchParams.get('copy'))
    : null;
  const useDefaults = searchParams.get('defaults') === '1';

  // Pull the existing checklist (edit mode) OR the source checklist
  // (create + ?copy=N). Both run through useChecklist; only one is enabled
  // at a time.
  const editQuery = useChecklist(editingId);
  const copyQuery = useChecklist(copySourceId);
  const loadingExisting = (isEdit && editQuery.isLoading) ||
    (copySourceId !== null && copyQuery.isLoading);
  const fetchError = (isEdit && editQuery.error) ||
    (copySourceId !== null && copyQuery.error);

  const initialValues = useMemo<FormValues>(() => {
    if (isEdit && editQuery.data) return checklistToForm(editQuery.data);
    if (copySourceId !== null && copyQuery.data)
      return checklistToCopyForm(copyQuery.data);
    if (useDefaults) return defaultsForm();
    return emptyForm();
    // initialValues is recomputed when its data sources change; reset() is
    // called from the load-effect below to push the new values into RHF.
  }, [isEdit, editQuery.data, copySourceId, copyQuery.data, useDefaults]);

  const {
    register,
    handleSubmit,
    control,
    reset,
    formState: { errors, isDirty, isSubmitting },
  } = useForm<FormValues>({
    defaultValues: emptyForm(),
  });

  const {
    fields: sectionFields,
    append: appendSection,
    remove: removeSection,
    move: moveSection,
  } = useFieldArray({ control, name: 'sections' });

  // Once the source data is loaded (or we know we're in blank/defaults mode),
  // push the initial values into RHF. This also clears isDirty so the dirty
  // guard doesn't fire immediately after load.
  const [initialised, setInitialised] = useState(false);
  useEffect(() => {
    if (initialised) return;
    if (isEdit && !editQuery.data) return;
    if (copySourceId !== null && !copyQuery.data) return;
    reset(initialValues);
    setInitialised(true);
  }, [initialised, isEdit, editQuery.data, copySourceId, copyQuery.data, initialValues, reset]);

  // Dirty-state browser-close guard.
  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [isDirty]);

  const [serverError, setServerError] = useState<string | null>(null);

  const saveMutation = useMutation<ChecklistFull, Error, ChecklistInputBody>({
    mutationFn: async (body) => {
      if (isEdit && editingId !== null) {
        return updateChecklist(editingId, body);
      }
      if (modelId === null) throw new Error('Model id is required to create a checklist');
      return createModelChecklist(modelId, body);
    },
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: ['model-checklists', modelId] });
      queryClient.invalidateQueries({ queryKey: ['checklist', saved.id] });
      queryClient.invalidateQueries({ queryKey: ['all-org-checklists'] });
      reset(checklistToForm(saved)); // clear dirty state before navigating
      navigate(`/aircraft-models/${modelId}`);
    },
    onError: (err) => {
      const msg = err.message ?? 'Failed to save checklist';
      // Backend returns 409 with text like "A normal_ops checklist already exists…"
      // — surface it as a friendlier inline message.
      if (msg.toLowerCase().includes('already exists')) {
        setServerError(
          'A checklist of this type already exists for this model. Edit the existing one or choose a different type.',
        );
      } else {
        setServerError(msg);
      }
    },
  });

  const onCancel = () => {
    if (isDirty && !window.confirm('Discard unsaved changes?')) return;
    if (modelId !== null) {
      navigate(`/aircraft-models/${modelId}`);
    } else {
      navigate('/aircraft-models');
    }
  };

  const onSubmit = handleSubmit((values) => {
    setServerError(null);
    if (values.sections.length === 0) {
      if (!window.confirm('This checklist has no sections. Save anyway?')) return;
    }

    const body: ChecklistInputBody = {
      type: values.type,
      title: values.title.trim(),
      revision: values.revision.trim() || '01',
      revision_date: values.revision_date ? values.revision_date : null,
      sections: values.sections.map((s, si) => ({
        title: s.title.trim(),
        sort_order: si + 1,
        start_on_new_page: s.start_on_new_page,
        items: s.items.map((it, ii) => ({
          label: it.label.trim(),
          value: it.value,
          responsibility: it.responsibility,
          sort_order: ii + 1,
        })),
      })),
    };

    saveMutation.mutate(body);
  });

  if (modelId === null) {
    return <p style={{ padding: 'var(--space-5)' }}>Invalid URL: missing model id.</p>;
  }
  if (loadingExisting) {
    return <p style={{ padding: 'var(--space-5)' }}>Loading checklist…</p>;
  }
  if (fetchError) {
    return (
      <p style={{ padding: 'var(--space-5)', color: 'var(--color-critical)' }}>
        Failed to load checklist: {(fetchError as Error).message}
      </p>
    );
  }

  const pageTitle = isEdit ? 'Edit checklist' : 'New checklist';

  return (
    <form className={styles.page} onSubmit={onSubmit}>
      <div className={styles.headerBar}>
        <button
          type="button"
          className={styles.btnSecondary}
          onClick={onCancel}
          aria-label="Cancel and return"
        >
          <ArrowLeft size={14} /> Back
        </button>
        <div className={styles.headerLeft}>
          <h1 className={styles.title}>{pageTitle}</h1>
          <div className={styles.subtitle}>
            {isDirty && <span className={styles.dirtyDot} aria-hidden="true" />}
            {isDirty ? 'Unsaved changes' : 'No changes'}
          </div>
        </div>
        <div className={styles.headerActions}>
          <button type="button" className={styles.btnSecondary} onClick={onCancel}>
            Cancel
          </button>
          <button
            type="submit"
            className={styles.btnPrimary}
            disabled={!isDirty || isSubmitting || saveMutation.isPending}
          >
            {saveMutation.isPending
              ? 'Saving…'
              : isEdit
                ? 'Save changes'
                : 'Create checklist'}
          </button>
        </div>
      </div>

      <div className={styles.body}>
        {serverError && <div className={styles.errorBanner}>{serverError}</div>}

        <div className={styles.headerFields}>
          <div className={styles.field}>
            <label className={styles.label}>Title *</label>
            <input
              className={`${styles.input} ${errors.title ? styles.inputError : ''}`}
              placeholder="e.g. Normal Operations"
              {...register('title', { required: 'Title is required', validate: (v) => v.trim().length > 0 || 'Title is required' })}
            />
            {errors.title && (
              <span className={styles.errorText}>{errors.title.message}</span>
            )}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Type *</label>
            {isEdit ? (
              <span
                className={styles.typeBadge}
                style={{
                  background: 'color-mix(in srgb, var(--color-text-muted) 12%, transparent)',
                  color: 'var(--color-text-muted)',
                  borderColor: 'var(--color-border)',
                }}
                title="Type is immutable once a checklist is created"
              >
                {/* The form value is set by reset(); use a static string read from the registered value */}
                <TypeReadOnly control={control} />
              </span>
            ) : (
              <select className={styles.select} {...register('type')}>
                {TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t === 'normal_ops' ? 'Normal Ops' : 'QRH'}
                  </option>
                ))}
              </select>
            )}
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Revision</label>
            <input
              className={styles.input}
              placeholder="01"
              {...register('revision')}
            />
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Revision date</label>
            <Controller
              name="revision_date"
              control={control}
              render={({ field }) => (
                <IsoDateInput
                  className={styles.input}
                  value={field.value ?? ''}
                  onChange={(e) => field.onChange(e.target.value)}
                  onBlur={field.onBlur}
                  name={field.name}
                />
              )}
            />
          </div>
        </div>

        {sectionFields.length === 0 && (
          <div className={styles.emptyHint}>
            No sections yet. Add one below to start building your checklist.
          </div>
        )}

        {sectionFields.map((field, sectionIndex) => (
          <SectionCard
            key={field.id}
            control={control}
            register={register}
            errors={errors}
            sectionIndex={sectionIndex}
            isFirst={sectionIndex === 0}
            isLast={sectionIndex === sectionFields.length - 1}
            onMoveUp={() => moveSection(sectionIndex, sectionIndex - 1)}
            onMoveDown={() => moveSection(sectionIndex, sectionIndex + 1)}
            onRemove={() => removeSection(sectionIndex)}
          />
        ))}

        <button
          type="button"
          className={styles.addButton}
          onClick={() =>
            appendSection({
              title: '',
              start_on_new_page: false,
              items: [],
            })
          }
        >
          <Plus size={14} /> Add section
        </button>
      </div>
    </form>
  );
}

// Read-only type display for edit mode. Uses Control.getValues so we don't
// re-register the field as an input (which would let the user mutate it).
function TypeReadOnly({ control }: { control: Control<FormValues> }) {
  // useWatch keeps the render in sync with whatever the form holds, which is
  // what reset() pushed when the existing checklist loaded.
  const v = (control._formValues.type ?? 'normal_ops') as ChecklistType;
  return <>{v === 'normal_ops' ? 'Normal Ops' : 'QRH'}</>;
}

// ---------------------------------------------------------------------------
// Section card
// ---------------------------------------------------------------------------

interface SectionCardProps {
  control: Control<FormValues>;
  register: UseFormRegister<FormValues>;
  errors: FieldErrors<FormValues>;
  sectionIndex: number;
  isFirst: boolean;
  isLast: boolean;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}

function SectionCard({
  control,
  register,
  errors,
  sectionIndex,
  isFirst,
  isLast,
  onMoveUp,
  onMoveDown,
  onRemove,
}: SectionCardProps) {
  const {
    fields: itemFields,
    append: appendItem,
    remove: removeItem,
    move: moveItem,
  } = useFieldArray({ control, name: `sections.${sectionIndex}.items` });

  const sectionTitleError = errors.sections?.[sectionIndex]?.title;

  return (
    <div className={styles.sectionCard}>
      <div className={styles.sectionHeader}>
        <div className={styles.sectionHeaderLeft}>
          <input
            className={`${styles.sectionTitleInput} ${
              sectionTitleError ? styles.inputError : ''
            }`}
            placeholder="Section title (e.g. Preflight)"
            {...register(`sections.${sectionIndex}.title`, {
              required: 'Section title is required',
              validate: (v) => v.trim().length > 0 || 'Section title is required',
            })}
          />
          {sectionTitleError && (
            <span className={styles.errorText}>{sectionTitleError.message}</span>
          )}
        </div>
        <button
          type="button"
          className={styles.iconButton}
          disabled={isFirst}
          onClick={onMoveUp}
          aria-label="Move section up"
          title="Move up"
        >
          <ChevronUp size={14} />
        </button>
        <button
          type="button"
          className={styles.iconButton}
          disabled={isLast}
          onClick={onMoveDown}
          aria-label="Move section down"
          title="Move down"
        >
          <ChevronDown size={14} />
        </button>
        <button
          type="button"
          className={`${styles.iconButton} ${styles.iconButtonDanger}`}
          onClick={() => {
            if (window.confirm('Remove this section and all its items?')) {
              onRemove();
            }
          }}
          aria-label="Remove section"
          title="Remove section"
        >
          <Trash2 size={14} />
        </button>
      </div>

      <div className={styles.sectionFlags}>
        <label className={styles.checkbox}>
          <input
            type="checkbox"
            {...register(`sections.${sectionIndex}.start_on_new_page`)}
          />
          Start on a new page in PDF export
        </label>
      </div>

      <div className={styles.itemList}>
        {itemFields.length === 0 && (
          <div
            style={{
              fontSize: '0.82rem',
              color: 'var(--color-text-muted)',
              padding: 'var(--space-2)',
            }}
          >
            No items in this section yet.
          </div>
        )}
        {itemFields.map((field, itemIndex) => (
          <ItemRow
            key={field.id}
            sectionIndex={sectionIndex}
            itemIndex={itemIndex}
            register={register}
            errors={errors}
            isFirst={itemIndex === 0}
            isLast={itemIndex === itemFields.length - 1}
            onMoveUp={() => moveItem(itemIndex, itemIndex - 1)}
            onMoveDown={() => moveItem(itemIndex, itemIndex + 1)}
            onRemove={() => removeItem(itemIndex)}
          />
        ))}
      </div>

      <button
        type="button"
        className={styles.addButton}
        onClick={() =>
          appendItem({
            label: '',
            value: '',
            responsibility: 'PF',
          })
        }
      >
        <Plus size={14} /> Add item
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Item row
// ---------------------------------------------------------------------------

interface ItemRowProps {
  sectionIndex: number;
  itemIndex: number;
  register: UseFormRegister<FormValues>;
  errors: FieldErrors<FormValues>;
  isFirst: boolean;
  isLast: boolean;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}

function ItemRow({
  sectionIndex,
  itemIndex,
  register,
  errors,
  isFirst,
  isLast,
  onMoveUp,
  onMoveDown,
  onRemove,
}: ItemRowProps) {
  const labelError = errors.sections?.[sectionIndex]?.items?.[itemIndex]?.label;
  return (
    <div className={styles.itemRow}>
      <input
        className={`${styles.input} ${labelError ? styles.inputError : ''}`}
        placeholder="Label (e.g. Propellers)"
        {...register(`sections.${sectionIndex}.items.${itemIndex}.label`, {
          required: 'Item label is required',
          validate: (v) => v.trim().length > 0 || 'Item label is required',
        })}
      />
      <input
        className={styles.input}
        placeholder="Value / expected response (e.g. Inspect and confirm secure)"
        {...register(`sections.${sectionIndex}.items.${itemIndex}.value`)}
      />
      <select
        className={styles.select}
        {...register(`sections.${sectionIndex}.items.${itemIndex}.responsibility`)}
      >
        {RESPONSIBILITIES.map((r) => (
          <option key={r} value={r}>
            {r}
          </option>
        ))}
      </select>
      <button
        type="button"
        className={styles.iconButton}
        disabled={isFirst}
        onClick={onMoveUp}
        aria-label="Move item up"
        title="Move up"
      >
        <ChevronUp size={14} />
      </button>
      <button
        type="button"
        className={styles.iconButton}
        disabled={isLast}
        onClick={onMoveDown}
        aria-label="Move item down"
        title="Move down"
      >
        <ChevronDown size={14} />
      </button>
      <button
        type="button"
        className={`${styles.iconButton} ${styles.iconButtonDanger}`}
        onClick={onRemove}
        aria-label="Remove item"
        title="Remove item"
      >
        <Trash2 size={14} />
      </button>
    </div>
  );
}
