// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import { Bar, BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import {
  useBatteryCycleDistribution,
  useFleetStatsOverview,
} from '../../hooks/useFleetStats';
import type { BatteryModelComparison } from '../../types/fleetStats';
import { EmptyState } from './EmptyState';
import styles from './fleetStatsDashboard.module.css';

function ModelRow({ m }: { m: BatteryModelComparison }) {
  const specLabel =
    m.nominal_max_cycles !== null && m.cycle_uncertainty !== null
      ? `nominal ${m.nominal_max_cycles} ± ${m.cycle_uncertainty}`
      : 'no spec';
  return (
    <tr className={styles.widgetTableRow}>
      <td>
        <Link
          to={`/fleet-stats/batteries/model/${encodeURIComponent(m.model)}`}
          style={{ color: 'inherit', textDecoration: 'none' }}
        >
          {m.model}
        </Link>
      </td>
      <td className={`${styles.cellRight} ${styles.cellMono}`}>{m.battery_count}</td>
      <td className={`${styles.cellRight} ${styles.cellMono}`}>
        {m.avg_cycle_count.toFixed(0)}
      </td>
      <td className={`${styles.cellRight} ${styles.cellMuted}`}>{specLabel}</td>
    </tr>
  );
}

export function BatteriesTab() {
  const { data: overview, isLoading: overviewLoading } = useFleetStatsOverview();
  const { data: distribution, isLoading: distLoading } = useBatteryCycleDistribution();

  return (
    <>
      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Battery models</h2>
          <span className={styles.sectionMeta}>aggregated across the fleet</span>
        </header>
        {overviewLoading && (
          <p style={{ color: 'var(--color-text-secondary)' }}>Loading…</p>
        )}
        {!overviewLoading && overview && overview.battery_model_summary.length === 0 && (
          <EmptyState message="No active batteries with a compatible model set." />
        )}
        {overview && overview.battery_model_summary.length > 0 && (
          <table className={styles.widgetTable}>
            <thead className={styles.widgetTableHead}>
              <tr>
                <th>Model</th>
                <th className={styles.cellRight}>Count</th>
                <th className={styles.cellRight}>Avg cycles</th>
                <th className={styles.cellRight}>Spec</th>
              </tr>
            </thead>
            <tbody>
              {overview.battery_model_summary.map((m) => (
                <ModelRow key={m.model} m={m} />
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Cycle health distribution</h2>
          <span className={styles.sectionMeta}>
            {distribution ? `${distribution.total_active} active batteries` : ''}
          </span>
        </header>
        {distLoading && <p style={{ color: 'var(--color-text-secondary)' }}>Loading…</p>}
        {!distLoading && distribution && distribution.total_active === 0 && (
          <EmptyState message="No active batteries with cycle data." />
        )}
        {distribution && distribution.total_active > 0 && (
          <div className={styles.histogramBox}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={distribution.buckets} margin={{ top: 8, right: 8, bottom: 8, left: 8 }}>
                <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" />
                <XAxis
                  dataKey="label"
                  stroke="var(--color-text-muted)"
                  fontSize={11}
                  tickLine={false}
                />
                <YAxis
                  allowDecimals={false}
                  stroke="var(--color-text-muted)"
                  fontSize={11}
                  tickLine={false}
                />
                <Tooltip
                  contentStyle={{
                    background: 'var(--color-bg-elevated)',
                    border: '1px solid var(--color-border-strong)',
                    color: 'var(--color-text-primary)',
                    fontSize: '0.8rem',
                  }}
                  formatter={(v: number) => [`${v} batteries`, 'count']}
                  labelFormatter={(l: string) => `Cycle ratio ${l}`}
                />
                <Bar dataKey="count" fill="var(--color-accent)" radius={[3, 3, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </section>

    </>
  );
}
