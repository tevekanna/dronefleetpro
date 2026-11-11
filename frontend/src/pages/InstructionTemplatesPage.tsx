// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Plus } from 'lucide-react';
import { fetchAircraftModels } from '../api/aircraftModels';
import { fetchInstructionTemplates } from '../api/maintenanceInstructionTemplates';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { CreateTemplateModal } from '../components/maintenance/CreateTemplateModal';
import { InstructionTemplateList } from '../components/maintenance/InstructionTemplateList';
import { showToast } from '../store/toastStore';
import type { TemplateListItem } from '../types/maintenanceInstructionTemplate';
import { MaintenanceSubNav } from '../components/maintenance/MaintenanceSubNav';
import styles from './InstructionTemplatesPage.module.css';

const GENERIC_KEY = 'generic';

type TemplateGroup = {
  key: string;
  label: string;
  items: TemplateListItem[];
};

export function InstructionTemplatesPage() {
  const [searchParams] = useSearchParams();
  const initialModel = searchParams.get('model');
  const [modelFilter, setModelFilter] = useState<number | null>(
    initialModel ? Number(initialModel) : null,
  );
  const [showCreateModal, setShowCreateModal] = useState(false);
  const { data: currentUser } = useCurrentUser();
  const isAdmin = userHasAnyRole(currentUser, 'admin');
  const queryClient = useQueryClient();

  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'all'],
    queryFn: () => fetchAircraftModels(),
  });

  const filters = useMemo(
    () => (modelFilter !== null ? { aircraft_model_id: modelFilter } : {}),
    [modelFilter],
  );

  const {
    data: templates,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['instruction-templates', filters],
    queryFn: () => fetchInstructionTemplates(filters),
  });

  const modelNameById = useMemo(() => {
    const map = new Map<number, string>();
    for (const m of models ?? []) map.set(m.id, m.name);
    return map;
  }, [models]);

  const groups: TemplateGroup[] = useMemo(() => {
    const buckets = new Map<string, TemplateListItem[]>();
    for (const t of templates ?? []) {
      const key = t.aircraft_model_id === null ? GENERIC_KEY : String(t.aircraft_model_id);
      const list = buckets.get(key) ?? [];
      list.push(t);
      buckets.set(key, list);
    }
    const out: TemplateGroup[] = [];
    for (const [key, items] of buckets.entries()) {
      const label =
        key === GENERIC_KEY
          ? 'Generic'
          : modelNameById.get(Number(key)) ?? `Model #${key}`;
      items.sort((a, b) => a.name.localeCompare(b.name));
      out.push({ key, label, items });
    }
    out.sort((a, b) => {
      // Keep Generic last; otherwise alphabetical by model name.
      if (a.key === GENERIC_KEY) return 1;
      if (b.key === GENERIC_KEY) return -1;
      return a.label.localeCompare(b.label);
    });
    return out;
  }, [templates, modelNameById]);

  return (
    <div className={styles.page}>
      <MaintenanceSubNav />
      <div className={styles.header}>
        <h1 className={styles.title}>Inspection Checklists</h1>
        {isAdmin && (
          <button
            style={{
              display: 'inline-flex', alignItems: 'center', gap: 'var(--space-2)',
              background: 'var(--color-accent)', color: 'var(--color-text-inverse)',
              border: 'none', borderRadius: 'var(--radius-sm)',
              padding: 'var(--space-2) var(--space-4)',
              fontSize: '0.85rem', fontWeight: 500, cursor: 'pointer',
            }}
            onClick={() => setShowCreateModal(true)}
          >
            <Plus size={16} />
            New Template
          </button>
        )}
      </div>

      <p className={styles.subtitle}>
        Manufacturer and custom inspection checklists linked to aircraft models. Each
        template defines the exact inspection steps required by EASA OSO #3.
      </p>
      <p className={styles.subtitle}>
        Per-model defaults. For day-to-day work on a specific aircraft, use the
        aircraft's Model &amp; checklists tab.
      </p>

      <div className={styles.filters}>
        <select
          className={styles.select}
          value={modelFilter ?? ''}
          onChange={(e) => setModelFilter(e.target.value ? Number(e.target.value) : null)}
          aria-label="Filter by aircraft model"
        >
          <option value="">All models</option>
          {(models ?? []).map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </select>
      </div>

      {isLoading && (
        <div>
          <div className={styles.skeleton} />
          <div className={styles.skeleton} />
          <div className={styles.skeleton} />
        </div>
      )}

      {error && (
        <p className={styles.empty}>
          Failed to load inspection checklists: {(error as Error).message}
        </p>
      )}

      {!isLoading && !error && groups.length === 0 && (
        <div className={styles.empty}>No templates match this filter.</div>
      )}

      {!isLoading &&
        !error &&
        groups.map((group) => (
          <div key={group.key} className={styles.group}>
            <div className={styles.groupLabel}>{group.label}</div>
            <InstructionTemplateList templates={group.items} isAdmin={isAdmin} />
          </div>
        ))}

      {showCreateModal && (
        <CreateTemplateModal
          onClose={() => setShowCreateModal(false)}
          onSuccess={() => {
            queryClient.invalidateQueries({ queryKey: ['instruction-templates'] });
            showToast('Inspection template created.', 'success');
            setShowCreateModal(false);
          }}
        />
      )}
    </div>
  );
}
