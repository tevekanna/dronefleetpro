// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { AlertTriangle } from 'lucide-react';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { useControllerMaintenance, useControllerStats } from '../hooks/useFleetStats';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { AddMaintenanceModal } from '../components/maintenance/AddMaintenanceModal';
import { formatDaysAgo, formatDuration } from './fleetStatsFormat';
import styles from './FleetStatsShared.module.css';

export function ControllerStatsPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const numericId = id ? Number(id) : null;
  const { data, isLoading, error } = useControllerStats(numericId);
  const maintenanceQuery = useControllerMaintenance(numericId);
  const { data: currentUser } = useCurrentUser();
  const canAddMaintenance = userHasAnyRole(currentUser, 'admin', 'technician');
  const [showAddMaint, setShowAddMaint] = useState(false);

  const chartData = useMemo(() => {
    if (!data) return [];
    return data.usage_by_aircraft.map((u) => ({
      label: u.aircraft_equipment_id ?? u.aircraft_serial,
      hours: Math.round((u.flight_seconds / 3600) * 10) / 10,
    }));
  }, [data]);

  if (isLoading) return <p className={styles.loadingMsg}>Loading…</p>;
  if (error) return <p className={styles.errorMsg}>Failed: {(error as Error).message}</p>;
  if (!data) return null;

  return (
    <div className={styles.page}>
      <Breadcrumbs
        items={[
          { label: 'Fleet Statistics', to: '/fleet-stats' },
          { label: `Controller ${data.equipment_id ?? data.serial_number}` },
        ]}
      />

      <div className={styles.header}>
        <div>
          <h1 className={styles.title}>
            {data.equipment_id ?? data.serial_number}{' '}
            <span className={styles.muted} style={{ fontSize: '0.9rem' }}>
              — {data.model ?? 'unknown model'}
            </span>
          </h1>
          <div className={styles.description}>
            Serial {data.serial_number} · Status{' '}
            <span className={`${styles.pill} ${data.status === 'retired' ? styles.pillCritical : styles.pillOk}`}>
              {data.status}
            </span>
          </div>
        </div>
      </div>

      {data.inactivity_warning && (
        <div
          className={`${styles.section}`}
          style={{
            borderColor: 'var(--color-warning)',
            background: 'color-mix(in srgb, var(--color-warning) 10%, transparent)',
            display: 'flex',
            alignItems: 'center',
            gap: 'var(--space-2)',
            padding: 'var(--space-3) var(--space-4)',
          }}
        >
          <AlertTriangle size={18} color="var(--color-warning)" />
          <span>
            Inactivity warning: controller has been unused for {data.days_since_last_use} days.
            If retained, consider verifying battery/firmware before next flight.
          </span>
        </div>
      )}

      <div className={styles.statCards}>
        <div className={styles.statCard}>
          <div className={styles.statValue}>{data.total_flights}</div>
          <div className={styles.statLabel}>Total flights</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue}>{formatDuration(data.total_flight_seconds)}</div>
          <div className={styles.statLabel}>Total flight time</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue} style={{ fontSize: '1.1rem' }}>
            {data.first_used ? data.first_used.slice(0, 10) : '—'}
          </div>
          <div className={styles.statLabel}>First used</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue} style={{ fontSize: '1.1rem' }}>
            {formatDaysAgo(data.last_used)}
          </div>
          <div className={styles.statLabel}>Last used</div>
        </div>
      </div>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Flight time by paired aircraft</h2>
        {data.usage_by_aircraft.length === 0 ? (
          <div className={styles.empty}>
            This controller has not been linked to any flights yet.
          </div>
        ) : (
          <div className={styles.chartBox}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={chartData} layout="vertical">
                <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" />
                <XAxis type="number" stroke="var(--color-text-muted)" fontSize={11} unit="h" />
                <YAxis
                  type="category"
                  dataKey="label"
                  stroke="var(--color-text-muted)"
                  fontSize={11}
                  width={100}
                />
                <Tooltip
                  contentStyle={{
                    background: 'var(--color-bg-elevated)',
                    border: '1px solid var(--color-border-strong)',
                    color: 'var(--color-text-primary)',
                    fontSize: '0.8rem',
                  }}
                  formatter={(v: number) => [`${v} h`, 'Flight time']}
                />
                <Bar dataKey="hours" fill="#0ea5e9" />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </section>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Per-aircraft usage</h2>
        {data.usage_by_aircraft.length === 0 ? (
          <div className={styles.empty}>No aircraft pairings to show.</div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.th}>Equipment</th>
                <th className={styles.th}>Aircraft model</th>
                <th className={styles.th}>Flights</th>
                <th className={styles.th}>Flight time</th>
                <th className={styles.th}>Last flight</th>
              </tr>
            </thead>
            <tbody>
              {data.usage_by_aircraft.map((u) => (
                <tr key={u.aircraft_id} className={styles.tr}>
                  <td className={`${styles.td} ${styles.mono}`}>
                    {u.aircraft_equipment_id ?? u.aircraft_serial}
                  </td>
                  <td className={styles.td}>{u.aircraft_model ?? '—'}</td>
                  <td className={styles.td}>{u.flight_count}</td>
                  <td className={styles.td}>{formatDuration(u.flight_seconds)}</td>
                  <td className={styles.td}>{formatDaysAgo(u.last_flight)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className={styles.section}>
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: 'var(--space-3)',
          }}
        >
          <h2 className={styles.sectionTitle} style={{ marginBottom: 0 }}>
            Maintenance history
          </h2>
          {canAddMaintenance && (
            <button
              type="button"
              onClick={() => setShowAddMaint(true)}
              style={{
                background: 'var(--color-accent)',
                color: 'var(--color-text-inverse)',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                padding: 'var(--space-2) var(--space-4)',
                fontSize: '0.85rem',
                fontWeight: 500,
                cursor: 'pointer',
              }}
            >
              Add maintenance event
            </button>
          )}
        </div>
        {maintenanceQuery.isLoading ? (
          <p style={{ color: 'var(--color-text-secondary)' }}>Loading maintenance history…</p>
        ) : maintenanceQuery.error ? (
          <p style={{ color: 'var(--color-critical)' }}>
            Failed to load maintenance: {(maintenanceQuery.error as Error).message}
          </p>
        ) : !maintenanceQuery.data || maintenanceQuery.data.length === 0 ? (
          <div className={styles.empty}>
            No maintenance events recorded for this controller yet.
          </div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.th}>Date</th>
                <th className={styles.th}>Category</th>
                <th className={styles.th}>Type</th>
                <th className={styles.th}>Action taken</th>
                <th className={styles.th}>Created by</th>
                <th className={styles.th}>Sign-off</th>
                <th className={styles.th}>Flight</th>
              </tr>
            </thead>
            <tbody>
              {maintenanceQuery.data.map((m) => {
                const action = m.action_taken ?? '';
                const truncated = action.length > 80 ? action.slice(0, 80) + '…' : action;
                return (
                  <tr
                    key={m.id}
                    className={styles.tr}
                    onClick={() => navigate(`/maintenance/${m.id}`)}
                    style={{ cursor: 'pointer' }}
                  >
                    <td className={`${styles.td} ${styles.mono}`}>
                      {m.performed_date.substring(0, 10)}
                    </td>
                    <td className={styles.td}>{m.maint_category ?? '—'}</td>
                    <td className={styles.td}>{m.maint_type ?? '—'}</td>
                    <td className={styles.td} title={action}>
                      {truncated || '—'}
                    </td>
                    <td className={`${styles.td} ${styles.mono}`}>{m.created_by}</td>
                    <td className={`${styles.td} ${styles.mono}`}>{m.sign_off}</td>
                    <td className={styles.td}>
                      {m.flight_id ? (
                        <span
                          onClick={(e) => {
                            e.stopPropagation();
                            navigate(`/flights/${m.flight_id}`);
                          }}
                          style={{
                            color: 'var(--color-accent)',
                            textDecoration: 'underline',
                          }}
                        >
                          Flight #{m.flight_id}
                        </span>
                      ) : (
                        '—'
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      {showAddMaint && numericId !== null && (
        <AddMaintenanceModal
          target={{
            kind: 'controller',
            controller: {
              id: data.controller_id,
              serial_number: data.serial_number,
              equipment_id: data.equipment_id,
              model: data.model,
              aircraft_id: null,
            },
          }}
          onClose={() => setShowAddMaint(false)}
        />
      )}
    </div>
  );
}
