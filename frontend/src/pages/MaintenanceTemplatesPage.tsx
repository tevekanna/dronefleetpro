// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Plus } from 'lucide-react';
import { fetchAircraftModels } from '../api/aircraftModels';
import type { AircraftModelWithCount } from '../api/aircraftModels';
import { ModelAccordion } from '../components/shared/ModelAccordion';
import type { AccordionGroup } from '../components/shared/ModelAccordion';
import { TemplateRow } from '../components/maintenance-templates/TemplateRow';
import { TemplateFormModal } from '../components/maintenance-templates/TemplateFormModal';
import { SyncTemplateModal } from '../components/maintenance-templates/SyncTemplateModal';
import { DeleteTemplateConfirm } from '../components/maintenance-templates/DeleteTemplateConfirm';
import { PermanentDeleteTemplateConfirm } from '../components/maintenance-templates/PermanentDeleteTemplateConfirm';
import { useScheduleTemplates } from '../hooks/useMaintenanceScheduleTemplates';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import type { MaintenanceScheduleTemplate } from '../types/maintenanceScheduleTemplate';
import { MaintenanceSubNav } from '../components/maintenance/MaintenanceSubNav';
import styles from './MaintenanceTemplatesPage.module.css';

export function MaintenanceTemplatesPage() {
  const { data: currentUser } = useCurrentUser();
  const isAdmin = userHasAnyRole(currentUser, 'admin');

  const [searchParams] = useSearchParams();
  const initialModel = searchParams.get('model');
  const [modelFilter, setModelFilter] = useState<number | null>(
    initialModel ? Number(initialModel) : null,
  );
  const [showInactive, setShowInactive] = useState(false);

  const [creating, setCreating] = useState<{ defaultModelId?: number } | null>(null);
  const [editing, setEditing] = useState<MaintenanceScheduleTemplate | null>(null);
  const [syncing, setSyncing] = useState<MaintenanceScheduleTemplate | null>(null);
  const [deleting, setDeleting] = useState<MaintenanceScheduleTemplate | null>(null);
  const [permanentDeleting, setPermanentDeleting] =
    useState<MaintenanceScheduleTemplate | null>(null);

  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'all'],
    queryFn: () => fetchAircraftModels(),
  });

  const queryParams = useMemo(
    () => ({
      ...(modelFilter !== null ? { model_id: modelFilter } : {}),
      ...(showInactive ? { include_inactive: true } : {}),
    }),
    [modelFilter, showInactive],
  );
  const { data: templates, isLoading, error } = useScheduleTemplates(queryParams);

  const modelMap = useMemo(() => {
    const m = new Map<number, AircraftModelWithCount>();
    for (const am of models ?? []) m.set(am.id, am);
    return m;
  }, [models]);

  const groups: AccordionGroup[] = useMemo(() => {
    const buckets = new Map<number, MaintenanceScheduleTemplate[]>();
    for (const t of templates ?? []) {
      const list = buckets.get(t.model_id) ?? [];
      list.push(t);
      buckets.set(t.model_id, list);
    }
    const out: { modelId: number; label: string; items: MaintenanceScheduleTemplate[] }[] = [];
    for (const [modelId, items] of buckets.entries()) {
      const m = modelMap.get(modelId);
      const label = m?.name ?? `Model #${modelId}`;
      items.sort((a, b) => a.name.localeCompare(b.name));
      out.push({ modelId, label, items });
    }
    out.sort((a, b) => a.label.localeCompare(b.label));

    return out.map((g) => ({
      groupKey: String(g.modelId),
      label: g.label,
      count: g.items.length,
      children: (
        <div className={styles.rowList}>
          {g.items.map((t) => (
            <TemplateRow
              key={t.id}
              template={t}
              canEdit={isAdmin}
              onEdit={(tt) => setEditing(tt)}
              onSync={(tt) => setSyncing(tt)}
              onDelete={(tt) => setDeleting(tt)}
              onPermanentDelete={(tt) => setPermanentDeleting(tt)}
            />
          ))}
          {isAdmin && (
            <button
              type="button"
              className={styles.addInGroup}
              onClick={() => setCreating({ defaultModelId: g.modelId })}
            >
              <Plus size={14} /> Add template for {g.label}
            </button>
          )}
        </div>
      ),
    }));
  }, [templates, modelMap, isAdmin]);

  return (
    <div className={styles.page}>
      <MaintenanceSubNav />
      <div className={styles.header}>
        <h1 className={styles.title}>Schedule Templates</h1>
        {isAdmin && (
          <button
            type="button"
            className={styles.addButton}
            onClick={() => setCreating({})}
            disabled={!models || models.length === 0}
          >
            <Plus size={16} /> Add Template
          </button>
        )}
      </div>

      <p className={styles.subtitle}>
        Per-model defaults that spawn maintenance schedules whenever a new aircraft of
        that model is registered. Editing a template does not touch existing schedules
        — use Sync to push your changes to per-aircraft rows.
      </p>
      <p className={styles.subtitle}>
        Per-model defaults. For day-to-day work on a specific aircraft, use the
        aircraft's Model &amp; checklists tab.
      </p>

      <div className={styles.filters}>
        <select
          className={styles.select}
          value={modelFilter ?? ''}
          onChange={(e) =>
            setModelFilter(e.target.value ? Number(e.target.value) : null)
          }
          aria-label="Filter by aircraft model"
        >
          <option value="">All models</option>
          {(models ?? []).map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </select>
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={showInactive}
            onChange={(e) => setShowInactive(e.target.checked)}
          />
          Show inactive
        </label>
      </div>

      {isLoading && (
        <div>
          <div className={styles.skeletonRow} />
          <div className={styles.skeletonRow} />
          <div className={styles.skeletonRow} />
        </div>
      )}

      {error && (
        <p className={styles.errorMsg}>
          Failed to load templates: {(error as Error).message}
        </p>
      )}

      {!isLoading && !error && groups.length === 0 && (
        <div className={styles.empty}>
          <span className={styles.emptyTitle}>No templates match this filter.</span>
          <span className={styles.emptyHint}>
            Try clearing the model filter, toggling "Show inactive", or adding a new
            template.
          </span>
        </div>
      )}

      {!isLoading && !error && groups.length > 0 && (
        <ModelAccordion
          groups={groups}
          storageKey="maintenance-templates"
          defaultOpen
        />
      )}

      {creating && models && (
        <TemplateFormModal
          isOpen
          onClose={() => setCreating(null)}
          models={models}
          defaultModelId={creating.defaultModelId}
        />
      )}
      {editing && models && (
        <TemplateFormModal
          isOpen
          onClose={() => setEditing(null)}
          models={models}
          existingTemplate={editing}
        />
      )}
      {syncing && (
        <SyncTemplateModal
          isOpen
          onClose={() => setSyncing(null)}
          template={syncing}
          modelName={modelMap.get(syncing.model_id)?.name ?? `Model #${syncing.model_id}`}
        />
      )}
      {deleting && (
        <DeleteTemplateConfirm
          isOpen
          onClose={() => setDeleting(null)}
          template={deleting}
        />
      )}
      {permanentDeleting && (
        <PermanentDeleteTemplateConfirm
          isOpen
          onClose={() => setPermanentDeleting(null)}
          template={permanentDeleting}
        />
      )}
    </div>
  );
}
