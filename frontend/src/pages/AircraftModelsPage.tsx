// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useQuery, useQueryClient, useMutation } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { Plus, Pencil, GitMerge, Archive, ArchiveRestore, Trash2 } from 'lucide-react';
import {
  fetchAircraftModels,
  permanentDeleteAircraftModel,
  retireAircraftModel,
  updateAircraftModel,
  type AircraftModelWithStats,
} from '../api/aircraftModels';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { ModelFormModal } from '../components/aircraft-models/ModelFormModal';
import { MergeModelModal } from '../components/aircraft-models/MergeModelModal';
import { formatDuration } from './fleetStatsFormat';
import retireStyles from '../components/shared/RetireModal.module.css';
import styles from './UserManagementPage.module.css';

/**
 * Per-org aircraft models catalog with aggregate flight stats. List view
 * is open to all roles (the underlying GET /aircraft-models endpoint is
 * authenticated-any). Mutations (Add / Edit / Merge / Retire / Delete)
 * stay admin-only — both gated on the per-button level here and rejected
 * at 403 by the backend.
 */
export function AircraftModelsPage() {
  const { data: currentUser, isLoading: loadingUser } = useCurrentUser();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { data, isLoading, error } = useQuery({
    queryKey: ['aircraft-models', 'all'],
    queryFn: () => fetchAircraftModels(),
  });

  const isAdmin = userHasAnyRole(currentUser, 'admin');

  const [showAdd, setShowAdd] = useState(false);
  const [editModel, setEditModel] = useState<AircraftModelWithStats | null>(null);
  const [mergeFrom, setMergeFrom] = useState<AircraftModelWithStats | null>(null);
  const [pendingDelete, setPendingDelete] = useState<AircraftModelWithStats | null>(null);

  const toggleActive = useMutation<void, Error, AircraftModelWithStats>({
    mutationFn: async (m: AircraftModelWithStats) => {
      if (m.is_active) {
        await retireAircraftModel(m.id);
      } else {
        await updateAircraftModel(m.id, { is_active: true });
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['aircraft-models'] }),
  });

  if (loadingUser) return <p className={styles.loading}>Loading...</p>;

  if (isLoading) return <div className={styles.loading}>Loading aircraft models…</div>;
  if (error) return <div className={styles.error}>Failed to load aircraft models: {(error as Error).message}</div>;

  const rows = data ?? [];

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>Aircraft Models</h1>
        {isAdmin && (
          <button className={styles.btnAdd} onClick={() => setShowAdd(true)}>
            <Plus size={16} /> Add Model
          </button>
        )}
      </div>

      {rows.length === 0 ? (
        <div className={styles.empty}>
          <div className={styles.emptyTitle}>No aircraft models yet</div>
          <div className={styles.emptyHint}>
            Add a model so pilots can pick it when registering aircraft.
          </div>
        </div>
      ) : (
        <div className={styles.tableWrapper}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th className={styles.th}>Name</th>
              <th className={styles.th}>Manufacturer</th>
              <th className={styles.th}>Category</th>
              <th className={styles.th}>Individuals</th>
              <th className={styles.th}>Status breakdown</th>
              <th className={styles.th}>Total flights</th>
              <th className={styles.th}>Total flight time</th>
              <th className={styles.th}>Last flight</th>
              <th className={styles.th}>Status</th>
              {isAdmin && (
                <th className={styles.th} style={{ width: 1, whiteSpace: 'nowrap', minWidth: 160 }}>Actions</th>
              )}
            </tr>
          </thead>
          <tbody>
            {rows.map(m => {
              const stopRow = (e: React.MouseEvent) => e.stopPropagation();
              return (
                <tr
                  key={m.id}
                  className={m.is_active ? '' : styles.inactiveRow}
                  onClick={() => navigate(`/aircraft-models/${m.id}`)}
                  style={{ cursor: 'pointer' }}
                  role="button"
                  tabIndex={0}
                  aria-label={`Open detail for ${m.name}`}
                  onKeyDown={e => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      navigate(`/aircraft-models/${m.id}`);
                    }
                  }}
                >
                  <td className={styles.td}>
                    <span className={styles.mono}>{m.name}</span>
                  </td>
                  <td className={styles.td}>{m.manufacturer ?? '—'}</td>
                  <td className={styles.td}>{m.category ?? '—'}</td>
                  <td className={styles.td}>
                    <span className={styles.mono}>{m.aircraft_count}</span>
                  </td>
                  <td className={styles.td} style={{ whiteSpace: 'nowrap' }}>
                    <span style={{ color: 'var(--color-ok)' }}>{m.active_count}</span>
                    {' · '}
                    <span style={{ color: 'var(--color-warning)' }}>{m.maintenance_count}</span>
                    {' · '}
                    <span style={{ color: 'var(--color-text-muted)' }}>{m.retired_count}</span>
                  </td>
                  <td className={styles.td}>
                    <span className={styles.mono}>{m.total_flights}</span>
                  </td>
                  <td className={styles.td}>{formatDuration(m.total_flight_seconds)}</td>
                  <td className={styles.td}>
                    {m.last_flight ? m.last_flight.substring(0, 10) : '—'}
                  </td>
                  <td className={styles.td}>
                    <span className={styles.statusDot}>
                      <span className={`${styles.dot} ${m.is_active ? styles.dotActive : styles.dotInactive}`} />
                      {m.is_active ? 'active' : 'retired'}
                    </span>
                  </td>
                  {isAdmin && (
                    <td className={styles.td} style={{ whiteSpace: 'nowrap', minWidth: 160 }} onClick={stopRow}>
                      <button
                        className={styles.btnEdit}
                        onClick={() => setEditModel(m)}
                        aria-label={`Edit ${m.name}`}
                        title="Edit"
                      >
                        <Pencil size={14} />
                      </button>
                      <button
                        className={styles.btnEdit}
                        onClick={() => setMergeFrom(m)}
                        disabled={!m.is_active}
                        aria-label={`Merge ${m.name}`}
                        title={m.is_active ? 'Merge into another model' : 'Retired models cannot be merged'}
                      >
                        <GitMerge size={14} />
                      </button>
                      <button
                        className={styles.btnEdit}
                        onClick={() => toggleActive.mutate(m)}
                        aria-label={m.is_active ? `Retire ${m.name}` : `Unretire ${m.name}`}
                        title={m.is_active ? 'Retire (set inactive)' : 'Unretire (set active)'}
                      >
                        {m.is_active ? <Archive size={14} /> : <ArchiveRestore size={14} />}
                      </button>
                      <button
                        className={styles.btnEdit}
                        onClick={() => setPendingDelete(m)}
                        disabled={m.aircraft_count > 0}
                        aria-label={`Permanently delete ${m.name}`}
                        title={
                          m.aircraft_count > 0
                            ? `Cannot delete — ${m.aircraft_count} aircraft use this model`
                            : 'Permanently delete'
                        }
                      >
                        <Trash2 size={14} />
                      </button>
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
        </div>
      )}

      {showAdd && <ModelFormModal onClose={() => setShowAdd(false)} />}
      {editModel && <ModelFormModal model={editModel} onClose={() => setEditModel(null)} />}
      {mergeFrom && (
        <MergeModelModal
          source={mergeFrom}
          candidates={rows.filter(x => x.is_active && x.id !== mergeFrom.id)}
          onClose={() => setMergeFrom(null)}
        />
      )}
      {pendingDelete && (
        <PermanentDeleteModal
          model={pendingDelete}
          onClose={() => setPendingDelete(null)}
        />
      )}
    </div>
  );
}

interface PermanentDeleteModalProps {
  model: AircraftModelWithStats;
  onClose: () => void;
}

function PermanentDeleteModal({ model, onClose }: PermanentDeleteModalProps) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation<void, Error, number>({
    mutationFn: async (id: number) => {
      await permanentDeleteAircraftModel(id);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['aircraft-models'] });
      onClose();
    },
    onError: (e: Error) => {
      setError(e.message);
    },
  });

  return (
    <div className={retireStyles.overlay} onClick={onClose}>
      <div className={retireStyles.modal} onClick={e => e.stopPropagation()}>
        <h2 className={retireStyles.title}>Permanently delete model</h2>
        <p className={retireStyles.body}>
          This will permanently delete the model <strong>«{model.name}»</strong>. This action
          cannot be undone. No aircraft currently use this model.
        </p>
        {error && (
          <p style={{
            color: 'var(--color-critical)',
            fontSize: '0.85rem',
            marginBottom: 'var(--space-4)',
            fontFamily: 'var(--font-body)',
          }}>
            {error}
          </p>
        )}
        <div className={retireStyles.actions}>
          <button
            className={retireStyles.btnCancel}
            onClick={onClose}
            disabled={mutation.isPending}
          >
            Cancel
          </button>
          <button
            className={retireStyles.btnRetire}
            onClick={() => { setError(null); mutation.mutate(model.id); }}
            disabled={mutation.isPending}
          >
            {mutation.isPending ? 'Deleting...' : 'Delete permanently'}
          </button>
        </div>
      </div>
    </div>
  );
}
