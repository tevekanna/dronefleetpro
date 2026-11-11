// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchPurgePreflight, purgeData } from '../../api/purge';
import type { PurgeConfirmation, PurgeTarget } from '../../api/purge';
import { PurgeConfirmModal } from './PurgeConfirmModal';
import styles from './DataPurgeSection.module.css';

interface PurgeItem {
  target: PurgeTarget;
  label: string;
  description: string;
  queryKeys: string[];
}

const PURGE_ITEMS: PurgeItem[] = [
  {
    target: 'flights',
    label: 'Flight Logs',
    description: 'All flight records, trajectories, and imported flight logs',
    queryKeys: ['flights', 'aircraft'],
  },
  {
    target: 'aircraft',
    label: 'Aircraft',
    description: 'All aircraft profiles and their associated maintenance history',
    queryKeys: ['aircraft'],
  },
  {
    target: 'batteries',
    label: 'Batteries',
    description: 'All battery profiles and cycle history',
    queryKeys: ['batteries'],
  },
  {
    target: 'controllers',
    label: 'Controllers',
    description: 'All remote controller profiles',
    queryKeys: ['controllers'],
  },
  {
    target: 'pilots',
    label: 'Pilots',
    description: 'All pilot profiles and qualifications',
    queryKeys: ['pilots'],
  },
  {
    target: 'incidents',
    label: 'Incidents',
    description: 'All incident reports',
    queryKeys: ['incidents'],
  },
];

export function DataPurgeSection() {
  const queryClient = useQueryClient();
  const [activeItem, setActiveItem] = useState<PurgeItem | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [deletedCount, setDeletedCount] = useState<number | undefined>(undefined);
  const [purgeError, setPurgeError] = useState<string | null>(null);

  const activeTarget = activeItem?.target ?? null;
  const preflight = useQuery({
    queryKey: ['purge-preflight', activeTarget],
    queryFn: () => fetchPurgePreflight(activeTarget as PurgeTarget),
    enabled: activeTarget !== null,
  });

  const handleConfirm = async (confirmation?: PurgeConfirmation) => {
    if (!activeItem) return;
    setIsSubmitting(true);
    setPurgeError(null);
    try {
      const result = await purgeData(activeItem.target, confirmation);
      setDeletedCount(result.deleted);
      for (const key of activeItem.queryKeys) {
        queryClient.invalidateQueries({ queryKey: [key] });
      }
      queryClient.invalidateQueries({ queryKey: ['purge-preflight', activeItem.target] });
    } catch (e) {
      // Do NOT set deletedCount here — a failed purge must never render
      // the success view.
      setPurgeError(e instanceof Error ? e.message : 'Purge failed');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    setActiveItem(null);
    setDeletedCount(undefined);
    setPurgeError(null);
  };

  return (
    <div className={styles.section}>
      <h2 className={styles.title}>Danger Zone</h2>
      <p className={styles.warning}>
        The following actions permanently delete data for your organisation.
        These actions cannot be undone. Deleted data cannot be recovered.
      </p>
      <div className={styles.list}>
        {PURGE_ITEMS.map(item => (
          <div key={item.target} className={styles.item}>
            <div className={styles.itemInfo}>
              <span className={styles.itemName}>{item.label}</span>
              <span className={styles.itemDesc}>{item.description}</span>
            </div>
            <button
              className={styles.btnDeleteAll}
              onClick={() => { setDeletedCount(undefined); setPurgeError(null); setActiveItem(item); }}
            >
              Delete all
            </button>
          </div>
        ))}
      </div>
      <PurgeConfirmModal
        isOpen={activeItem !== null}
        entityLabel={activeItem?.label ?? ''}
        impact={preflight.data ?? null}
        isLoadingImpact={preflight.isLoading}
        errorMessage={
          purgeError ?? (preflight.error ? (preflight.error as Error).message : null)
        }
        onConfirm={handleConfirm}
        onCancel={handleClose}
        isSubmitting={isSubmitting}
        deletedCount={deletedCount}
      />
    </div>
  );
}
