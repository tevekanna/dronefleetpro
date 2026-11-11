// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { PilotDetailBreakdowns } from '../../api/pilotDetail';
import { BreakdownGroup } from '../breakdowns/BreakdownGroup';
import styles from '../../pages/PilotDetailPage.module.css';

/// Pilot activity-breakdowns section. Delegates each bar group to the
/// shared <BreakdownGroup> (extracted in PR #7 Commit 3 so the aircraft
/// detail page can reuse it).
export function PilotBreakdownsPanel({
  breakdowns,
}: {
  breakdowns: PilotDetailBreakdowns;
}) {
  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Activity breakdowns</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-5)' }}>
        <BreakdownGroup heading="EASA category" rows={breakdowns.by_easa_category} />
        <BreakdownGroup heading="Visual regime" rows={breakdowns.by_visual_regime} />
        <BreakdownGroup heading="Day / night" rows={breakdowns.by_daylight} />
        <BreakdownGroup heading="Aircraft model" rows={breakdowns.by_aircraft_model} />
        <BreakdownGroup heading="Mission type" rows={breakdowns.by_mission_type} />
      </div>
    </div>
  );
}
