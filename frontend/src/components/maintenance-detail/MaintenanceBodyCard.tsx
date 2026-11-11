// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { MaintenanceRecord } from '../../api/maintenance';
import styles from '../../pages/MaintenanceDetailPage.module.css';

interface Props {
  record: MaintenanceRecord;
}

function TextBlock({ value }: { value: string | null }) {
  const trimmed = value?.trim();
  if (!trimmed) {
    return <span className={styles.detailMuted}>(not recorded)</span>;
  }
  return <div className={styles.remarksBlock}>{trimmed}</div>;
}

export function MaintenanceBodyCard({ record }: Props) {
  return (
    <>
      <div className={styles.section}>
        <div className={styles.sectionHeading}>Observation</div>
        <TextBlock value={record.remark} />
      </div>
      <div className={styles.section}>
        <div className={styles.sectionHeading}>Action taken</div>
        <TextBlock value={record.action_taken} />
      </div>
    </>
  );
}
