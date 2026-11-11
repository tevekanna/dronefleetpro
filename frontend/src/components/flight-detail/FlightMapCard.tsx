// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';
import { FlightMap } from '../map/FlightMap';
import type { Flight } from '../../types/flight';
import styles from '../../pages/FlightDetailPage.module.css';

interface FlightMapCardProps {
  flight: Flight;
}

interface SparkPoint {
  t: number;
  alt: number;
}

function downsample(points: SparkPoint[], maxPoints = 200): SparkPoint[] {
  if (points.length <= maxPoints) return points;
  const stride = Math.ceil(points.length / maxPoints);
  const out: SparkPoint[] = [];
  for (let i = 0; i < points.length; i += stride) out.push(points[i]);
  // Always keep the last point so the plot ends at the real touchdown.
  if (out[out.length - 1] !== points[points.length - 1]) {
    out.push(points[points.length - 1]);
  }
  return out;
}

function formatTimeAxis(t: number): string {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${String(s).padStart(2, '0')}`;
}

export function FlightMapCard({ flight }: FlightMapCardProps) {
  const trajectory = flight.trajectory_json;

  if (!trajectory || trajectory.length === 0) {
    return (
      <div className={styles.section}>
        <div className={styles.sectionHeading}>Flight path</div>
        <div className={styles.mapEmpty}>
          No GPS data — manual or partially logged flight.
        </div>
      </div>
    );
  }

  const sparkPoints: SparkPoint[] = downsample(
    trajectory.map(p => ({ t: p.t_sec, alt: p.alt }))
  );

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Flight path</div>
      <div className={styles.mapWrapper}>
        <FlightMap
          trajectory={trajectory}
          homeLat={flight.home_lat}
          homeLon={flight.home_lon}
          height="500px"
        />
      </div>
      <div className={styles.sparkline}>
        <div className={styles.sparklineLabel}>Altitude (m) over time</div>
        <ResponsiveContainer width="100%" height={120}>
          <LineChart
            data={sparkPoints}
            margin={{ top: 5, right: 12, left: 0, bottom: 0 }}
          >
            <XAxis
              dataKey="t"
              tickFormatter={formatTimeAxis}
              stroke="var(--color-text-muted)"
              fontSize={11}
            />
            <YAxis
              stroke="var(--color-text-muted)"
              fontSize={11}
              width={40}
            />
            <Tooltip
              contentStyle={{
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-strong)',
                borderRadius: '4px',
                fontSize: '0.8rem',
                color: 'var(--color-text-primary)',
              }}
              labelFormatter={(t: number) => `t = ${formatTimeAxis(t)}`}
              formatter={(value: number) => [`${value.toFixed(1)} m`, 'Altitude']}
            />
            <Line
              type="monotone"
              dataKey="alt"
              stroke="var(--color-accent)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
