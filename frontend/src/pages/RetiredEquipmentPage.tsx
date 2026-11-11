// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Navigate } from 'react-router-dom';
import { fetchAircraft, updateAircraft } from '../api/aircraft';
import { fetchBatteries, updateBattery } from '../api/batteries';
import { fetchControllers, updateController } from '../api/controllers';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import type { Aircraft } from '../types/aircraft';
import type { Battery } from '../types/battery';
import type { Controller } from '../types/controller';
import { MaintenanceSubNav } from '../components/maintenance/MaintenanceSubNav';
import styles from './RetiredEquipmentPage.module.css';

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '—';
  return dateStr.substring(0, 10);
}

export function RetiredEquipmentPage() {
  const { data: currentUser, isLoading: loadingUser } = useCurrentUser();
  const queryClient = useQueryClient();
  const { data: aircraft } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const { data: batteries } = useQuery({ queryKey: ['batteries', true], queryFn: () => fetchBatteries(true) });
  const { data: controllers } = useQuery({ queryKey: ['controllers', true], queryFn: () => fetchControllers(true) });

  if (loadingUser) return <p className={styles.loading}>Loading...</p>;
  if (!userHasAnyRole(currentUser, 'admin')) return <Navigate to="/" replace />;

  const retiredAircraft = (aircraft ?? []).filter(a => a.status === 'retired');
  const retiredBatteries = (batteries ?? []).filter(b => b.status === 'retired');
  const retiredControllers = (controllers ?? []).filter(c => c.status === 'retired');

  return (
    <div className={styles.page}>
      <MaintenanceSubNav />
      <h1 className={styles.title}>Retired Equipment</h1>
      <AircraftSection items={retiredAircraft} queryClient={queryClient} />
      <BatterySection items={retiredBatteries} queryClient={queryClient} />
      <ControllerSection items={retiredControllers} queryClient={queryClient} />
    </div>
  );
}

function AircraftSection({ items, queryClient }: { items: Aircraft[]; queryClient: ReturnType<typeof useQueryClient> }) {
  const reactivateMutation = useMutation({
    mutationFn: (id: number) => updateAircraft(id, { status: 'active' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['aircraft', true] }),
  });

  return (
    <div className={styles.section}>
      <h2 className={styles.sectionTitle}>Aircraft</h2>
      {items.length === 0 ? (
        <div className={styles.empty}><span className={styles.emptyText}>No retired aircraft</span></div>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th className={styles.th}>Equipment ID</th>
              <th className={styles.th}>Model</th>
              <th className={styles.th}>Serial Number</th>
              <th className={styles.th}>Retirement Notes</th>
              <th className={styles.th}>Date Retired</th>
              <th className={styles.th}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {items.map(a => (
              <tr key={a.id}>
                <td className={`${styles.td} ${styles.mono}`}>{a.equipment_id ?? '—'}</td>
                <td className={styles.td}>{a.model ?? '—'}</td>
                <td className={`${styles.td} ${styles.mono}`}>{a.serial_number}</td>
                <td className={`${styles.td} ${styles.notes}`}>{a.retirement_notes ?? '—'}</td>
                <td className={`${styles.td} ${styles.mono}`}>{formatDate(a.created_at)}</td>
                <td className={styles.td}>
                  <button
                    className={styles.btnReactivate}
                    onClick={() => reactivateMutation.mutate(a.id)}
                    disabled={reactivateMutation.isPending}
                  >
                    Reactivate
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function BatterySection({ items, queryClient }: { items: Battery[]; queryClient: ReturnType<typeof useQueryClient> }) {
  const reactivateMutation = useMutation({
    mutationFn: (id: number) => updateBattery(id, { status: 'active' } as Record<string, unknown>),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['batteries', true] }),
  });

  return (
    <div className={styles.section}>
      <h2 className={styles.sectionTitle}>Batteries</h2>
      {items.length === 0 ? (
        <div className={styles.empty}><span className={styles.emptyText}>No retired batteries</span></div>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th className={styles.th}>Equipment ID</th>
              <th className={styles.th}>Compatible Model</th>
              <th className={styles.th}>Serial Number</th>
              <th className={styles.th}>Total Cycles</th>
              <th className={styles.th}>Retirement Notes</th>
              <th className={styles.th}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {items.map(b => (
              <tr key={b.id}>
                <td className={`${styles.td} ${styles.mono}`}>{b.equipment_id ?? '—'}</td>
                <td className={styles.td}>{b.compatible_model ?? '—'}</td>
                <td className={`${styles.td} ${styles.mono}`}>{b.serial_number}</td>
                <td className={`${styles.td} ${styles.mono}`}>{b.cycle_count}</td>
                <td className={`${styles.td} ${styles.notes}`}>{b.retirement_notes ?? '—'}</td>
                <td className={styles.td}>
                  <button
                    className={styles.btnReactivate}
                    onClick={() => reactivateMutation.mutate(b.id)}
                    disabled={reactivateMutation.isPending}
                  >
                    Reactivate
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function ControllerSection({ items, queryClient }: { items: Controller[]; queryClient: ReturnType<typeof useQueryClient> }) {
  const reactivateMutation = useMutation({
    mutationFn: (id: number) => updateController(id, { status: 'active' } as Record<string, unknown>),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['controllers', true] }),
  });

  return (
    <div className={styles.section}>
      <h2 className={styles.sectionTitle}>Controllers</h2>
      {items.length === 0 ? (
        <div className={styles.empty}><span className={styles.emptyText}>No retired controllers</span></div>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th className={styles.th}>Equipment ID</th>
              <th className={styles.th}>Model</th>
              <th className={styles.th}>Compatible Models</th>
              <th className={styles.th}>Serial Number</th>
              <th className={styles.th}>Retirement Notes</th>
              <th className={styles.th}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {items.map(c => (
              <tr key={c.id}>
                <td className={`${styles.td} ${styles.mono}`}>{c.equipment_id ?? '—'}</td>
                <td className={styles.td}>{c.model ?? '—'}</td>
                <td className={styles.td}>{c.compatible_models ?? '—'}</td>
                <td className={`${styles.td} ${styles.mono}`}>{c.serial_number}</td>
                <td className={`${styles.td} ${styles.notes}`}>{c.retirement_notes ?? '—'}</td>
                <td className={styles.td}>
                  <button
                    className={styles.btnReactivate}
                    onClick={() => reactivateMutation.mutate(c.id)}
                    disabled={reactivateMutation.isPending}
                  >
                    Reactivate
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
