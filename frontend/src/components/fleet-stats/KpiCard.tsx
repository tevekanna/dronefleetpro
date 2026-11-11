// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import { Line, LineChart, ResponsiveContainer } from 'recharts';
import type { KpiCard as KpiCardData, KpiSeverity } from '../../types/fleetDashboard';
import styles from './fleetStatsDashboard.module.css';

function severityColor(severity: KpiSeverity): string {
  switch (severity) {
    case 'red':
      return 'var(--color-critical)';
    case 'amber':
      return 'var(--color-warning)';
    default:
      return 'var(--color-accent)';
  }
}

function valueClass(severity: KpiSeverity): string {
  switch (severity) {
    case 'red':
      return styles.kpiValueRed;
    case 'amber':
      return styles.kpiValueAmber;
    default:
      return '';
  }
}

function dotClass(severity: KpiSeverity): string | null {
  switch (severity) {
    case 'red':
      return styles.severityDotRed;
    case 'amber':
      return styles.severityDotAmber;
    default:
      return null;
  }
}

function formatValue(value: number, kind: 'int' | 'decimal'): string {
  if (!Number.isFinite(value)) return '—';
  if (kind === 'decimal') {
    return value.toFixed(1);
  }
  return Math.round(value).toString();
}

export interface KpiCardProps {
  label: string;
  data: KpiCardData;
  /** "decimal" formats with one decimal (flight hours). "int" rounds to integer. */
  format: 'int' | 'decimal';
  /** When provided, the card becomes a clickable link to this route. */
  to?: string;
}

export function KpiCard({ label, data, format, to }: KpiCardProps) {
  const color = severityColor(data.severity);
  const sparkData = data.sparkline.map((v, i) => ({ i, v }));
  const dot = dotClass(data.severity);

  const body = (
    <>
      <div className={styles.kpiHeader}>
        <span className={styles.kpiLabel}>{label}</span>
        {dot && <span className={`${styles.severityDot} ${dot}`} aria-hidden="true" />}
      </div>
      <div className={`${styles.kpiValue} ${valueClass(data.severity)}`}>
        {formatValue(data.value, format)}
      </div>
      {data.secondary && <div className={styles.kpiSecondary}>{data.secondary}</div>}
      <div className={styles.kpiSparkline} aria-hidden="true">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={sparkData} margin={{ top: 6, right: 0, bottom: 0, left: 0 }}>
            <Line
              type="monotone"
              dataKey="v"
              stroke={color}
              strokeWidth={1.5}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </>
  );

  if (to) {
    return (
      <Link to={to} className={`${styles.kpiCard} ${styles.kpiCardClickable}`}>
        {body}
      </Link>
    );
  }

  return <div className={styles.kpiCard}>{body}</div>;
}
