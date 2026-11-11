// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { FlightFinding } from '../../api/flightDetail';
import { SeverityBadge } from '../shared/SeverityBadge';
import pageStyles from '../../pages/FlightDetailPage.module.css';
import styles from './FlightFindingsCard.module.css';

/// Formats a whole-second duration as "5m 38s" / "43s".
function formatDuration(seconds: number): string {
  const total = Math.round(Math.abs(seconds));
  const mins = Math.floor(total / 60);
  const secs = total % 60;
  return mins === 0 ? `${secs}s` : `${mins}m ${secs}s`;
}

/// Turns an arm-relative offset into a phrase a pilot reads without decoding a
/// sign: negative = before arming (pre-arm ground events), positive = into
/// flight. Null offsets (no timestamp) render nothing.
function offsetPhrase(seconds: number | null): string | null {
  if (seconds === null) return null;
  if (Math.round(seconds) === 0) return 'at arming';
  return seconds < 0
    ? `${formatDuration(seconds)} before arming`
    : `${formatDuration(seconds)} into flight`;
}

/// Log findings extracted from the flight log (ArduPilot .bin today). Rendered
/// between the map and the EASA card on the flight detail page.
///
/// Hidden entirely when there are no findings: most flights (all DJI imports,
/// any clean .bin) have none, and an empty card on every flight is noise. This
/// component takes no hooks, so the early return is safe.
export function FlightFindingsCard({ findings }: { findings: FlightFinding[] }) {
  if (!findings || findings.length === 0) return null;

  // The backend already returns findings ordered by severity (critical → low)
  // then by offset — see handlers/flights/detail.rs. Render as-is; no client
  // re-sort.
  return (
    <div className={pageStyles.section}>
      <div className={pageStyles.sectionHeading}>Log findings</div>
      <ul className={styles.list}>
        {findings.map((f) => {
          const phrase = offsetPhrase(f.first_offset_seconds);
          return (
            <li
              key={f.id}
              className={`${styles.row} ${f.resolved ? styles.resolved : ''}`}
            >
              <SeverityBadge severity={f.severity} />
              <div className={styles.body}>
                <div className={styles.message}>{f.message}</div>
                <div className={styles.meta}>
                  {phrase && <span>{phrase}</span>}
                  {f.occurrence_count > 1 && (
                    <span>{f.occurrence_count} occurrences</span>
                  )}
                  {f.resolved && (
                    <span className={styles.resolvedTag}>✓ resolved</span>
                  )}
                </div>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
