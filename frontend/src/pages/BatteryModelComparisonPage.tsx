// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo } from 'react';
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
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { useBatteryModelComparison } from '../hooks/useFleetStats';
import { formatDaysAgo, formatDuration, formatNumber } from './fleetStatsFormat';
import styles from './FleetStatsShared.module.css';

export function BatteryModelComparisonPage() {
  const { model } = useParams<{ model: string }>();
  const modelName = model ? decodeURIComponent(model) : null;
  const { data, isLoading, error } = useBatteryModelComparison(modelName);
  const navigate = useNavigate();

  const histogramData = useMemo(() => {
    if (!data) return [];
    return data.cycle_distribution.map((b) => ({
      range: `${b.lower}–${b.upper}`,
      count: b.count,
    }));
  }, [data]);

  if (isLoading) return <p className={styles.loadingMsg}>Loading…</p>;
  if (error) return <p className={styles.errorMsg}>Failed: {(error as Error).message}</p>;
  if (!data) return null;

  const nominalLabel =
    data.nominal_max_cycles !== null && data.cycle_uncertainty !== null
      ? `Nominal lifespan: ${data.nominal_max_cycles} ± ${data.cycle_uncertainty} cycles`
      : 'No nominal spec registered for this model';
  const nominalDenom = data.nominal_max_cycles ?? 0;

  return (
    <div className={styles.page}>
      <Breadcrumbs
        items={[
          { label: 'Fleet Statistics', to: '/fleet-stats' },
          { label: 'Model comparison' },
        ]}
      />

      <div className={styles.header}>
        <div>
          <h1 className={styles.title}>{data.model}</h1>
          <div className={styles.description}>
            {data.battery_count} batteries · {nominalLabel}
          </div>
        </div>
      </div>

      <div className={styles.statCards}>
        <div className={styles.statCard}>
          <div className={styles.statValue}>{data.avg_cycle_count.toFixed(0)}</div>
          <div className={styles.statLabel}>Avg cycles</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue}>
            {data.min_cycle_count} – {data.max_cycle_count}
          </div>
          <div className={styles.statLabel}>Cycle range</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue}>
            {data.avg_discharge_per_flight !== null
              ? `${formatNumber(data.avg_discharge_per_flight, 1)}%`
              : '—'}
          </div>
          <div className={styles.statLabel}>Avg discharge/flight (90d)</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue}>
            {data.avg_cell_variance_v !== null
              ? `${formatNumber(data.avg_cell_variance_v, 3)} V`
              : '—'}
          </div>
          <div className={styles.statLabel}>Avg cell variance (90d)</div>
        </div>
      </div>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Cycle count distribution</h2>
        {data.battery_count === 0 ? (
          <div className={styles.empty}>No batteries of this model to compare.</div>
        ) : (
          <div className={styles.chartBox}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={histogramData}>
                <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" />
                <XAxis dataKey="range" stroke="var(--color-text-muted)" fontSize={11} />
                <YAxis
                  stroke="var(--color-text-muted)"
                  fontSize={11}
                  allowDecimals={false}
                />
                <Tooltip
                  contentStyle={{
                    background: 'var(--color-bg-elevated)',
                    border: '1px solid var(--color-border-strong)',
                    color: 'var(--color-text-primary)',
                    fontSize: '0.8rem',
                  }}
                />
                <Bar dataKey="count" fill="#0ea5e9" />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </section>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Batteries in this model</h2>
        {data.batteries.length === 0 ? (
          <div className={styles.empty}>No batteries.</div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.th}>Equipment ID</th>
                <th className={styles.th}>Serial</th>
                <th className={styles.th}>Cycles</th>
                <th className={styles.th}>% of nominal</th>
                <th className={styles.th}>Flights</th>
                <th className={styles.th}>Flight time</th>
                <th className={styles.th}>Last used</th>
              </tr>
            </thead>
            <tbody>
              {data.batteries.map((b) => {
                const pct =
                  nominalDenom > 0
                    ? `${((b.cycle_count / nominalDenom) * 100).toFixed(0)}%`
                    : '—';
                return (
                  <tr
                    key={b.battery_id}
                    className={styles.tr}
                    onClick={() => navigate(`/fleet-stats/batteries/${b.battery_id}`)}
                  >
                    <td className={`${styles.td} ${styles.mono}`}>{b.equipment_id ?? '—'}</td>
                    <td className={`${styles.td} ${styles.mono}`}>{b.serial_number}</td>
                    <td className={styles.td}>{b.cycle_count}</td>
                    <td className={styles.td}>{pct}</td>
                    <td className={styles.td}>{b.total_flights}</td>
                    <td className={styles.td}>{formatDuration(b.total_flight_seconds)}</td>
                    <td className={styles.td}>{formatDaysAgo(b.last_used)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
