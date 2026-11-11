// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { CalendarClock, ClipboardCheck } from 'lucide-react';
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
import { useAircraftModelStats } from '../hooks/useFleetStats';
import { DocumentsSection } from '../components/aircraft-models/DocumentsSection';
import { ChecklistsSection } from '../components/aircraft-models/ChecklistsSection';
import { formatDaysAgo, formatDate, formatDuration } from './fleetStatsFormat';
import styles from './FleetStatsShared.module.css';

export function AircraftModelDetailPage() {
  const { id } = useParams<{ id: string }>();
  const numericId = id ? Number(id) : null;
  const { data, isLoading, error } = useAircraftModelStats(numericId);
  const navigate = useNavigate();

  const chartData = useMemo(() => {
    if (!data) return [];
    return [...data.individuals]
      .sort((a, b) => b.total_seconds - a.total_seconds)
      .map((i) => ({
        label: i.equipment_id ?? i.serial_number,
        hours: Math.round((i.total_seconds / 3600) * 10) / 10,
      }));
  }, [data]);

  if (isLoading) return <p className={styles.loadingMsg}>Loading…</p>;
  if (error) return <p className={styles.errorMsg}>Failed: {(error as Error).message}</p>;
  if (!data) return null;

  const subline = [data.manufacturer, data.category].filter(Boolean).join(' · ');
  const lastFlightDisplay = data.last_flight ? formatDate(data.last_flight) : 'Never';

  // Regulatory weights + EASA class. Weight/dimension fields read "Not
  // recorded" when null; class marking reads "Not marked" because its
  // absence is a deliberate fact (many older airframes carry no marking).
  const specs: Array<{ label: string; value: string | null; placeholder: string }> = [
    { label: 'MTOM', value: data.mtom_kg != null ? `${data.mtom_kg} kg` : null, placeholder: 'Not recorded' },
    { label: 'Empty weight', value: data.empty_weight_kg != null ? `${data.empty_weight_kg} kg` : null, placeholder: 'Not recorded' },
    { label: 'Max payload', value: data.max_payload_kg != null ? `${data.max_payload_kg} kg` : null, placeholder: 'Not recorded' },
    { label: 'Max characteristic dimension', value: data.max_dimension_cm != null ? `${data.max_dimension_cm} cm` : null, placeholder: 'Not recorded' },
    { label: 'EASA class marking', value: data.class_marking, placeholder: 'Not marked' },
  ];

  return (
    <div className={styles.page}>
      <Breadcrumbs
        items={[
          { label: 'Aircraft Models', to: '/aircraft-models' },
          { label: data.name },
        ]}
      />

      <div className={styles.header}>
        <div>
          <h1 className={styles.title}>{data.name}</h1>
          {subline && <div className={styles.description}>{subline}</div>}
          <div
            className={styles.description}
            style={{ marginTop: 'var(--space-1)' }}
          >
            <span style={{ color: 'var(--color-ok)' }}>{data.active_count} active</span>
            {' · '}
            <span style={{ color: 'var(--color-warning)' }}>{data.maintenance_count} maintenance</span>
            {' · '}
            <span style={{ color: 'var(--color-text-muted)' }}>{data.retired_count} retired</span>
            {!data.is_active && (
              <>
                {' · '}
                <span style={{ color: 'var(--color-critical)' }}>model retired</span>
              </>
            )}
          </div>
        </div>
      </div>

      <div className={styles.actions}>
        <Link className={styles.btn} to={`/maintenance-templates?model=${data.id}`}>
          <CalendarClock size={14} aria-hidden /> Schedule templates
        </Link>
        <Link className={styles.btn} to={`/maintenance-instruction-templates?model=${data.id}`}>
          <ClipboardCheck size={14} aria-hidden /> Inspection checklists
        </Link>
      </div>

      <div className={styles.statCards}>
        <div className={styles.statCard}>
          <div className={styles.statValue}>{data.aircraft_count}</div>
          <div className={styles.statLabel}>Total individuals</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue}>{data.total_flights}</div>
          <div className={styles.statLabel}>Total flights</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue}>
            {formatDuration(data.total_flight_seconds)}
          </div>
          <div className={styles.statLabel}>Total flight time</div>
        </div>
        <div className={styles.statCard}>
          <div className={styles.statValue} style={{ fontSize: '1.1rem' }}>
            {lastFlightDisplay}
          </div>
          <div className={styles.statLabel}>Last flight</div>
        </div>
      </div>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Weights &amp; EASA class</h2>
        <table className={styles.table}>
          <tbody>
            {specs.map((s) => (
              <tr key={s.label}>
                <td className={`${styles.td} ${styles.muted}`}>{s.label}</td>
                <td className={`${styles.td} ${styles.mono}`}>
                  {s.value != null ? s.value : <span className={styles.muted}>{s.placeholder}</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <DocumentsSection modelId={data.id} modelName={data.name} />

      <ChecklistsSection modelId={data.id} modelName={data.name} />

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Flight time by individual</h2>
        {data.individuals.length === 0 ? (
          <div className={styles.empty}>No aircraft of this model yet.</div>
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
                  width={110}
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
        <h2 className={styles.sectionTitle}>Aircraft in this model</h2>
        {data.individuals.length === 0 ? (
          <div className={styles.empty}>No aircraft to show.</div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.th}>Equipment ID</th>
                <th className={styles.th}>Serial number</th>
                <th className={styles.th}>Status</th>
                <th className={styles.th}>Total flights</th>
                <th className={styles.th}>Total flight time</th>
                <th className={styles.th}>Last flight</th>
                <th className={styles.th}>Needs review</th>
              </tr>
            </thead>
            <tbody>
              {data.individuals.map((i) => (
                <tr
                  key={i.id}
                  className={styles.tr}
                  onClick={() => navigate(`/aircraft/${i.id}`)}
                  style={{ cursor: 'pointer' }}
                >
                  <td className={`${styles.td} ${styles.mono}`}>
                    {i.equipment_id ?? '—'}
                  </td>
                  <td className={`${styles.td} ${styles.mono}`}>{i.serial_number}</td>
                  <td className={styles.td}>
                    <span
                      className={`${styles.pill} ${
                        i.status === 'retired'
                          ? styles.pillCritical
                          : i.status === 'maintenance'
                            ? styles.pillWarning
                            : styles.pillOk
                      }`}
                    >
                      {i.status}
                    </span>
                  </td>
                  <td className={styles.td}>{i.total_flights}</td>
                  <td className={styles.td}>{formatDuration(i.total_seconds)}</td>
                  <td className={styles.td}>{formatDaysAgo(i.last_flight)}</td>
                  <td className={styles.td}>
                    {i.needs_manual_sn ? (
                      <span
                        title="No verified serial number — operator should correct this manually"
                        style={{ color: 'var(--color-warning)' }}
                      >
                        ⚠
                      </span>
                    ) : (
                      ''
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
