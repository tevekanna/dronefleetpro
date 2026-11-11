// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { ENTITY_GROUPS, type EntityGroup } from '../../types/dataTransfer';
import { humaniseBytes } from '../../utils/download';
import styles from './ExportSummaryModal.module.css';

// Keep in sync with CANONICAL_TABLES grouping in
// backend/src/services/data_transfer_schema.rs.
const TABLES_PER_GROUP: Record<EntityGroup, string[]> = {
  equipment:    ['aircraft_models', 'aircraft', 'aircraft_identifiers', 'batteries', 'battery_identifiers', 'remote_controllers', 'controller_identifiers'],
  pilots:       ['pilots', 'pilot_qualifications'],
  flights:      ['flights'],
  maintenance:  ['maintenance_schedule_templates', 'maintenance_instruction_templates', 'maintenance_checklist_items', 'maintenance_schedules', 'maintenance', 'maintenance_checklist_completions', 'maintenance_drafts', 'defect_log', 'battery_tests'],
  incidents:    ['incidents', 'incident_notes'],
  settings:     ['settings', 'drone_type_mappings', 'checklists'],
  organisation: ['organisation_branding'],
  audit_log:    ['audit_log'],
};

export type ExportFormat = 'json' | 'zip';

interface ExportSummaryModalProps {
  isOpen: boolean;
  selectedGroups: EntityGroup[];
  counts: Record<string, number>;
  estimatedSizeBytes: number;
  isSubmitting: boolean;
  onConfirm: (format: ExportFormat) => void;
  onCancel: () => void;
}

export function ExportSummaryModal({
  isOpen,
  selectedGroups,
  counts,
  estimatedSizeBytes,
  isSubmitting,
  onConfirm,
  onCancel,
}: ExportSummaryModalProps) {
  const [format, setFormat] = useState<ExportFormat>('json');

  if (!isOpen) return null;

  const groupRows = selectedGroups.map((g) => {
    const tables = TABLES_PER_GROUP[g];
    const rowCount = tables.reduce((sum, t) => sum + (counts[t] ?? 0), 0);
    const label = ENTITY_GROUPS.find((e) => e.id === g)?.label ?? g;
    return { id: g, label, rowCount };
  });
  const totalRows = groupRows.reduce((s, r) => s + r.rowCount, 0);
  const includesAuditLog = selectedGroups.includes('audit_log');

  return (
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>Confirm Export</h2>
        <p className={styles.body}>You are about to export the following data:</p>
        <ul className={styles.groupList}>
          {groupRows.map((r) => (
            <li key={r.id} className={styles.groupRow}>
              <span className={styles.groupLabel}>{r.label}</span>
              <span className={styles.groupCount}>
                {r.rowCount} {r.rowCount === 1 ? 'row' : 'rows'}
              </span>
            </li>
          ))}
        </ul>
        <div className={styles.totals}>
          <span>Total: {totalRows} rows</span>
          <span className={styles.totalSep}>·</span>
          <span>Estimated size: {humaniseBytes(estimatedSizeBytes)}</span>
        </div>
        {includesAuditLog && (
          <div className={styles.warning}>
            ⚠ The audit log is included. The export may be large.
          </div>
        )}
        <fieldset className={styles.formatField}>
          <legend className={styles.formatLegend}>Format</legend>
          <label className={styles.radioRow}>
            <input
              type="radio"
              name="export-format"
              value="json"
              checked={format === 'json'}
              onChange={() => setFormat('json')}
            />
            <span><strong>JSON</strong> — single .json file</span>
          </label>
          <label className={styles.radioRow}>
            <input
              type="radio"
              name="export-format"
              value="zip"
              checked={format === 'zip'}
              onChange={() => setFormat('zip')}
            />
            <span><strong>ZIP</strong> — compressed; faster download</span>
          </label>
          <p className={styles.browserNote}>
            Browsers may block ZIP downloads on HTTP servers. Click 'Allow' if prompted, or use JSON to avoid the warning.
          </p>
        </fieldset>
        <div className={styles.actions}>
          <button className={styles.btnCancel} onClick={onCancel} disabled={isSubmitting}>
            Cancel
          </button>
          <button
            className={styles.btnConfirm}
            onClick={() => onConfirm(format)}
            disabled={isSubmitting}
          >
            {isSubmitting ? 'Exporting...' : 'Export'}
          </button>
        </div>
      </div>
    </div>
  );
}
