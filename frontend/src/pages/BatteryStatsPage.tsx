// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useNavigate, useParams } from 'react-router-dom';
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { BatteryTestsSection } from '../components/fleet/BatteryTestsSection';
import { useBatteryStats } from '../hooks/useFleetStats';
import type { BatteryStatsDetail, HealthSample } from '../types/fleetStats';
import { formatDaysAgo, formatDuration, formatNumber } from './fleetStatsFormat';
import styles from './FleetStatsShared.module.css';

function statusTone(
  cycle_count: number,
  nominal_max: number | null,
): 'ok' | 'warning' | 'critical' {
  if (!nominal_max || nominal_max <= 0) return 'ok';
  if (cycle_count >= nominal_max) return 'critical';
  if (cycle_count >= nominal_max * 0.8) return 'warning';
  return 'ok';
}

function RemainingCell({ stats }: { stats: BatteryStatsDetail }) {
  const r = stats.estimated_remaining_cycles;
  if (!r) return <span className={styles.muted}>no spec</span>;
  if (r.mid === 0 && r.high === 0) {
    return <span style={{ color: 'var(--color-critical)' }}>past nominal lifespan</span>;
  }
  return (
    <span>
      {r.low}–{r.high}{' '}
      <span className={styles.muted}>cycles remaining (mid {r.mid})</span>
    </span>
  );
}

function chartData(series: HealthSample[], key: keyof HealthSample): Array<{ i: number; v: number | null; date: string }> {
  return series.map((s, i) => {
    const raw = s[key];
    const v = typeof raw === 'number' ? raw : null;
    const date = s.flight_datetime ? s.flight_datetime.slice(0, 10) : `#${s.flight_id}`;
    return { i, v, date };
  });
}

function HealthChart({
  title,
  series,
  field,
  unit,
  color,
  domain,
}: {
  title: string;
  series: HealthSample[];
  field: 'min_battery_pct' | 'battery_temp_c' | 'cell_variance_v';
  unit: string;
  color: string;
  domain?: [number, number];
}) {
  const data = chartData(series, field).filter((d) => d.v !== null);
  if (data.length === 0) {
    return (
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>{title}</h3>
        <div className={styles.empty}>No data available yet for this metric.</div>
      </div>
    );
  }
  return (
    <div className={styles.section}>
      <h3 className={styles.sectionTitle}>{title}</h3>
      <div className={styles.chartBox}>
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data}>
            <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" />
            <XAxis dataKey="date" stroke="var(--color-text-muted)" fontSize={11} />
            <YAxis stroke="var(--color-text-muted)" fontSize={11} domain={domain} />
            <Tooltip
              contentStyle={{
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-strong)',
                color: 'var(--color-text-primary)',
                fontSize: '0.8rem',
              }}
              formatter={(v: number) => [`${v.toFixed(2)} ${unit}`, title]}
            />
            <Line type="monotone" dataKey="v" stroke={color} strokeWidth={2} dot={false} />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

export function BatteryStatsPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const numericId = id ? Number(id) : null;
  const { data, isLoading, error } = useBatteryStats(numericId);

  if (isLoading) return <p className={styles.loadingMsg}>Loading…</p>;
  if (error) return <p className={styles.errorMsg}>Failed: {(error as Error).message}</p>;
  if (!data) return null;

  const tone = statusTone(data.cycle_count, data.nominal_max_cycles);
  const recentFlights = data.health_timeseries.slice(-20).reverse();

  return (
    <div className={styles.page}>
      <Breadcrumbs
        items={[
          { label: 'Fleet Statistics', to: '/fleet-stats' },
          { label: `Battery ${data.equipment_id ?? data.serial_number}` },
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

      <div className={styles.statCards}>
        <StatCard
          label="Cycle count"
          value={`${data.cycle_count}${data.nominal_max_cycles ? ` / ${data.nominal_max_cycles}` : ''}`}
          tone={tone}
        />
        <StatCard label="Remaining" valueNode={<RemainingCell stats={data} />} />
        <StatCard label="Total flights" value={data.total_flights} />
        <StatCard
          label="Total flight time"
          value={formatDuration(data.total_flight_seconds)}
        />
        <StatCard
          label="Avg discharge/flight"
          value={
            data.avg_discharge_per_flight !== null
              ? `${formatNumber(data.avg_discharge_per_flight, 1)}%`
              : '—'
          }
        />
        <StatCard label="Last used" value={formatDaysAgo(data.last_used)} />
      </div>

      <HealthChart
        title="Minimum battery % per flight"
        series={data.health_timeseries}
        field="min_battery_pct"
        unit="%"
        color="#0ea5e9"
        domain={[0, 100]}
      />
      <HealthChart
        title="Battery temperature per flight"
        series={data.health_timeseries}
        field="battery_temp_c"
        unit="°C"
        color="#f59e0b"
      />
      <HealthChart
        title="Cell variance per flight"
        series={data.health_timeseries}
        field="cell_variance_v"
        unit="V"
        color="#ef4444"
      />

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Recent flights</h2>
        {recentFlights.length === 0 ? (
          <div className={styles.empty}>No flights recorded for this battery.</div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.th}>Date</th>
                <th className={styles.th}>Duration</th>
                <th className={styles.th}>Min battery</th>
                <th className={styles.th}>Temp</th>
                <th className={styles.th}>Cell variance</th>
              </tr>
            </thead>
            <tbody>
              {recentFlights.map((f) => (
                <tr
                  key={f.flight_id}
                  className={styles.tr}
                  onClick={() => navigate(`/flights/${f.flight_id}`)}
                >
                  <td className={`${styles.td} ${styles.mono}`}>
                    {f.flight_datetime ? f.flight_datetime.slice(0, 10) : '—'}
                  </td>
                  <td className={styles.td}>{formatDuration(f.duration_seconds)}</td>
                  <td className={styles.td}>
                    {f.min_battery_pct !== null ? `${f.min_battery_pct}%` : '—'}
                  </td>
                  <td className={styles.td}>
                    {formatNumber(f.battery_temp_c, 1)}{f.battery_temp_c !== null ? ' °C' : ''}
                  </td>
                  <td className={styles.td}>
                    {formatNumber(f.cell_variance_v, 3)}{f.cell_variance_v !== null ? ' V' : ''}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <BatteryTestsSection batteryId={numericId!} nominalCapacityWh={null} />
    </div>
  );
}

function StatCard({
  label,
  value,
  valueNode,
  tone,
}: {
  label: string;
  value?: number | string;
  valueNode?: React.ReactNode;
  tone?: 'ok' | 'warning' | 'critical';
}) {
  const toneClass =
    tone === 'critical'
      ? styles.statValueCritical
      : tone === 'warning'
      ? styles.statValueWarning
      : tone === 'ok'
      ? styles.statValueOk
      : '';
  return (
    <div className={styles.statCard}>
      <div className={`${styles.statValue} ${toneClass}`}>
        {valueNode ?? value}
      </div>
      <div className={styles.statLabel}>{label}</div>
    </div>
  );
}
