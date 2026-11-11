// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { AlertTriangle, Plus } from 'lucide-react';
import { fetchAircraftModels } from '../../api/aircraftModels';
import { fetchInstructionTemplates } from '../../api/maintenanceInstructionTemplates';
import { useScheduleTemplates } from '../../hooks/useMaintenanceScheduleTemplates';
import { showToast } from '../../store/toastStore';
import { TemplateRow } from '../maintenance-templates/TemplateRow';
import { TemplateFormModal } from '../maintenance-templates/TemplateFormModal';
import { SyncTemplateModal } from '../maintenance-templates/SyncTemplateModal';
import { DeleteTemplateConfirm } from '../maintenance-templates/DeleteTemplateConfirm';
import { PermanentDeleteTemplateConfirm } from '../maintenance-templates/PermanentDeleteTemplateConfirm';
import { CreateTemplateModal } from '../maintenance/CreateTemplateModal';
import { InstructionTemplateList } from '../maintenance/InstructionTemplateList';
import type { MaintenanceScheduleTemplate } from '../../types/maintenanceScheduleTemplate';
import styles from './ModelChecklistsTab.module.css';

interface Props {
  modelId: number | null;
  modelName: string | null;
  isAdmin: boolean;
}

/**
 * Outer guard: with no model there is nothing model-scoped to manage, so we
 * render an empty state and skip every data hook. When a model is present the
 * inner body mounts and runs its hooks unconditionally (Rules of Hooks).
 */
export function ModelChecklistsTab({ modelId, modelName, isAdmin }: Props) {
  if (modelId == null) {
    return (
      <div className={styles.emptyState}>
        No model assigned — set a model in Fleet to manage templates and checklists.
      </div>
    );
  }
  return (
    <ModelChecklistsBody
      modelId={modelId}
      modelName={modelName ?? `Model #${modelId}`}
      isAdmin={isAdmin}
    />
  );
}

function ModelChecklistsBody({
  modelId,
  modelName,
  isAdmin,
}: {
  modelId: number;
  modelName: string;
  isAdmin: boolean;
}) {
  const queryClient = useQueryClient();
  const [showInactive, setShowInactive] = useState(false);

  // Schedule-template modal state (reused from MaintenanceTemplatesPage).
  const [creatingTemplate, setCreatingTemplate] = useState(false);
  const [editingTemplate, setEditingTemplate] = useState<MaintenanceScheduleTemplate | null>(null);
  const [syncingTemplate, setSyncingTemplate] = useState<MaintenanceScheduleTemplate | null>(null);
  const [deletingTemplate, setDeletingTemplate] = useState<MaintenanceScheduleTemplate | null>(null);
  const [permanentDeletingTemplate, setPermanentDeletingTemplate] =
    useState<MaintenanceScheduleTemplate | null>(null);

  // Inspection-checklist create modal state.
  const [showCreateChecklist, setShowCreateChecklist] = useState(false);

  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'all'],
    queryFn: () => fetchAircraftModels(),
  });

  const scheduleParams = useMemo(
    () => ({ model_id: modelId, ...(showInactive ? { include_inactive: true } : {}) }),
    [modelId, showInactive],
  );
  const {
    data: scheduleTemplates,
    isLoading: schedLoading,
    error: schedError,
  } = useScheduleTemplates(scheduleParams);

  const checklistFilters = useMemo(() => ({ aircraft_model_id: modelId }), [modelId]);
  const {
    data: checklistTemplates,
    isLoading: checklistLoading,
    error: checklistError,
  } = useQuery({
    queryKey: ['instruction-templates', checklistFilters],
    queryFn: () => fetchInstructionTemplates(checklistFilters),
  });

  return (
    <>
      <div className={styles.banner}>
        <AlertTriangle size={18} className={styles.bannerIcon} aria-hidden />
        <span>
          Changes on this tab apply to ALL {modelName} aircraft, not just this one.
        </span>
      </div>

      {/* ── Section 1: schedule templates ─────────────────────────────── */}
      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Schedule templates</h2>
          <div className={styles.sectionActions}>
            <label className={styles.toggle}>
              <input
                type="checkbox"
                checked={showInactive}
                onChange={(e) => setShowInactive(e.target.checked)}
              />
              Show inactive
            </label>
            {isAdmin && (
              <button
                type="button"
                className={styles.addBtn}
                onClick={() => setCreatingTemplate(true)}
                disabled={!models || models.length === 0}
              >
                <Plus size={14} /> Add template for {modelName}
              </button>
            )}
          </div>
        </header>

        {schedLoading && <div className={styles.muted}>Loading templates…</div>}
        {schedError && (
          <div className={styles.errorBox}>
            Failed to load templates: {(schedError as Error).message}
          </div>
        )}
        {!schedLoading && !schedError && (scheduleTemplates?.length ?? 0) === 0 && (
          <div className={styles.muted}>No schedule templates for this model yet.</div>
        )}
        {!schedLoading && !schedError && (scheduleTemplates?.length ?? 0) > 0 && (
          <div className={styles.rowList}>
            {scheduleTemplates!.map((t) => (
              <TemplateRow
                key={t.id}
                template={t}
                canEdit={isAdmin}
                onEdit={(tt) => setEditingTemplate(tt)}
                onSync={(tt) => setSyncingTemplate(tt)}
                onDelete={(tt) => setDeletingTemplate(tt)}
                onPermanentDelete={(tt) => setPermanentDeletingTemplate(tt)}
              />
            ))}
          </div>
        )}
      </section>

      {/* ── Section 2: inspection checklists ──────────────────────────── */}
      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Inspection checklists</h2>
          {isAdmin && (
            <button
              type="button"
              className={styles.addBtn}
              onClick={() => setShowCreateChecklist(true)}
            >
              <Plus size={14} /> New checklist
            </button>
          )}
        </header>

        {checklistLoading && <div className={styles.muted}>Loading checklists…</div>}
        {checklistError && (
          <div className={styles.errorBox}>
            Failed to load checklists: {(checklistError as Error).message}
          </div>
        )}
        {!checklistLoading && !checklistError && (checklistTemplates?.length ?? 0) === 0 && (
          <div className={styles.muted}>No inspection checklists for this model yet.</div>
        )}
        {!checklistLoading && !checklistError && (checklistTemplates?.length ?? 0) > 0 && (
          <div className={styles.rowList}>
            <InstructionTemplateList templates={checklistTemplates!} isAdmin={isAdmin} />
          </div>
        )}

        <p className={styles.hint}>
          Checklists are attached to a specific schedule from the Maintenance tab (edit a
          schedule to link one).
        </p>
      </section>

      {/* ── Schedule-template modals (fixed to this model) ────────────── */}
      {creatingTemplate && models && (
        <TemplateFormModal
          isOpen
          onClose={() => setCreatingTemplate(false)}
          models={models}
          defaultModelId={modelId}
        />
      )}
      {editingTemplate && models && (
        <TemplateFormModal
          isOpen
          onClose={() => setEditingTemplate(null)}
          models={models}
          existingTemplate={editingTemplate}
        />
      )}
      {syncingTemplate && (
        <SyncTemplateModal
          isOpen
          onClose={() => setSyncingTemplate(null)}
          template={syncingTemplate}
          modelName={modelName}
        />
      )}
      {deletingTemplate && (
        <DeleteTemplateConfirm
          isOpen
          onClose={() => setDeletingTemplate(null)}
          template={deletingTemplate}
        />
      )}
      {permanentDeletingTemplate && (
        <PermanentDeleteTemplateConfirm
          isOpen
          onClose={() => setPermanentDeletingTemplate(null)}
          template={permanentDeletingTemplate}
        />
      )}

      {/* ── Inspection-checklist create modal (model preselected) ─────── */}
      {showCreateChecklist && (
        <CreateTemplateModal
          defaultModelId={modelId}
          onClose={() => setShowCreateChecklist(false)}
          onSuccess={() => {
            void queryClient.invalidateQueries({ queryKey: ['instruction-templates'] });
            showToast('Inspection checklist created.', 'success');
            setShowCreateChecklist(false);
          }}
        />
      )}
    </>
  );
}
