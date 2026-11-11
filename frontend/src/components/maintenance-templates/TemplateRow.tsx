// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Archive, Pencil, RefreshCw, Trash2 } from 'lucide-react';
import type { MaintenanceScheduleTemplate } from '../../types/maintenanceScheduleTemplate';
import styles from './TemplateRow.module.css';

interface Props {
  template: MaintenanceScheduleTemplate;
  canEdit: boolean;
  onEdit?: (t: MaintenanceScheduleTemplate) => void;
  onSync?: (t: MaintenanceScheduleTemplate) => void;
  onDelete?: (t: MaintenanceScheduleTemplate) => void;
  onPermanentDelete?: (t: MaintenanceScheduleTemplate) => void;
}

const INTERVAL_LABEL: Record<MaintenanceScheduleTemplate['interval_type'], string> = {
  flight_hours: 'h',
  calendar_days: 'd',
  cycles: 'cycles',
};

function formatInterval(
  type: MaintenanceScheduleTemplate['interval_type'],
  value: number,
): string {
  const unit = INTERVAL_LABEL[type];
  // Whole numbers render without decimals; fractional with one decimal.
  const v = Number.isInteger(value) ? String(value) : value.toFixed(1);
  return `${v}${type === 'cycles' ? ' ' : ''}${unit}`;
}

export function TemplateRow({
  template,
  canEdit,
  onEdit,
  onSync,
  onDelete,
  onPermanentDelete,
}: Props) {
  const inactive = !template.is_active;
  const rowClass = `${styles.row} ${inactive ? styles.rowInactive : ''}`;
  const referenceCount = template.reference_count ?? 0;

  return (
    <div className={rowClass}>
      <div className={styles.left}>
        <div className={styles.nameLine}>
          <span className={styles.name}>{template.name}</span>
          {template.is_critical && (
            <span className={styles.criticalBadge}>Critical</span>
          )}
          {inactive && <span className={styles.inactiveBadge}>Inactive</span>}
        </div>
        {template.description && (
          <div className={styles.description}>{template.description}</div>
        )}
      </div>

      <div className={styles.middle}>
        <div className={styles.intervalPrimary}>
          {formatInterval(template.interval_type, template.interval_value)}
        </div>
        {template.secondary_interval_type && template.secondary_interval_value !== null && (
          <div className={styles.intervalSecondary}>
            +{' '}
            {formatInterval(
              template.secondary_interval_type,
              template.secondary_interval_value,
            )}
          </div>
        )}
        <div className={styles.threshold}>
          warn at {Math.round(template.warning_threshold * 100)}%
        </div>
      </div>

      {canEdit && (
        <div className={styles.actions}>
          <button
            type="button"
            className={styles.iconButton}
            onClick={() => onEdit?.(template)}
            aria-label="Edit template"
            title="Edit"
          >
            <Pencil size={15} />
          </button>
          <button
            type="button"
            className={styles.iconButton}
            onClick={() => onSync?.(template)}
            aria-label="Sync template to existing schedules"
            title="Sync to existing schedules"
            disabled={inactive}
          >
            <RefreshCw size={15} />
          </button>
          <button
            type="button"
            className={styles.iconButton}
            onClick={() => onDelete?.(template)}
            aria-label="Deactivate template"
            title="Deactivate"
            disabled={inactive}
          >
            <Archive size={15} />
          </button>
          <button
            type="button"
            className={`${styles.iconButton} ${styles.iconButtonDanger}`}
            onClick={() => onPermanentDelete?.(template)}
            aria-label="Permanently delete template"
            title={
              referenceCount > 0
                ? `Cannot delete — ${referenceCount} schedules use this template`
                : 'Permanently delete'
            }
            disabled={referenceCount > 0}
          >
            <Trash2 size={15} />
          </button>
        </div>
      )}
    </div>
  );
}
