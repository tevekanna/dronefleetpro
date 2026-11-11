// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { exportData, listRecentJobs, previewExport } from '../../api/dataTransfer';
import { ENTITY_GROUPS, type EntityGroup } from '../../types/dataTransfer';
import {
  downloadAsJson,
  downloadAsZip,
  filenameFromContentDisposition,
  humaniseBytes,
  relativeTime,
} from '../../utils/download';
import { ExportSummaryModal, type ExportFormat } from './ExportSummaryModal';
import styles from './ExportTab.module.css';

const DEBOUNCE_MS = 300;

export function ExportTab() {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<Set<EntityGroup>>(new Set());
  const [debounced, setDebounced] = useState<EntityGroup[]>([]);
  const [showModal, setShowModal] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounceTimer = useRef<number | null>(null);

  // Debounce selection → trigger preview.
  useEffect(() => {
    if (debounceTimer.current) window.clearTimeout(debounceTimer.current);
    debounceTimer.current = window.setTimeout(() => {
      setDebounced(Array.from(selected));
    }, DEBOUNCE_MS);
    return () => {
      if (debounceTimer.current) window.clearTimeout(debounceTimer.current);
    };
  }, [selected]);

  const previewQuery = useQuery({
    queryKey: ['exportPreview', debounced.slice().sort().join(',')],
    queryFn: () => previewExport(debounced),
    enabled: debounced.length > 0,
    staleTime: 30 * 1000,
  });

  const jobsQuery = useQuery({
    queryKey: ['dataTransferJobs'],
    queryFn: () => listRecentJobs(15),
    staleTime: 10 * 1000,
  });

  const allChecked = selected.size === ENTITY_GROUPS.length;
  const noneChecked = selected.size === 0;

  const toggle = (g: EntityGroup) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(g)) next.delete(g);
      else next.add(g);
      return next;
    });
  };

  const toggleAll = () => {
    if (allChecked) setSelected(new Set());
    else setSelected(new Set(ENTITY_GROUPS.map((e) => e.id)));
  };

  const counts = previewQuery.data?.counts ?? {};
  const estimatedSize = previewQuery.data?.estimated_size_bytes ?? 0;
  const totalRows = useMemo(
    () => Object.values(counts).reduce((s, n) => s + (n ?? 0), 0),
    [counts],
  );

  const onExportClick = () => {
    if (noneChecked) return;
    setError(null);
    setShowModal(true);
  };

  const onConfirmExport = async (format: ExportFormat) => {
    setExporting(true);
    setError(null);
    try {
      const groups = Array.from(selected);
      const res = await exportData(groups);
      const filename = filenameFromContentDisposition(res.headers.get('content-disposition'));
      const blob = await res.blob();
      if (format === 'zip') {
        await downloadAsZip(blob, filename);
      } else {
        downloadAsJson(blob, filename);
      }
      setShowModal(false);
      queryClient.invalidateQueries({ queryKey: ['dataTransferJobs'] });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Export failed');
    } finally {
      setExporting(false);
    }
  };

  const exportJobs = (jobsQuery.data ?? []).filter((j) => j.job_type === 'export');

  return (
    <div className={styles.tab}>
      {/* Section 1: Entity selection */}
      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Choose what to export</h3>
        <label className={styles.selectAll}>
          <input
            type="checkbox"
            checked={allChecked}
            onChange={toggleAll}
            ref={(el) => {
              if (el) el.indeterminate = !allChecked && !noneChecked;
            }}
          />
          <span>Select all</span>
        </label>
        <div className={styles.groupGrid}>
          {ENTITY_GROUPS.map((g) => (
            <label key={g.id} className={styles.groupCard}>
              <input
                type="checkbox"
                checked={selected.has(g.id)}
                onChange={() => toggle(g.id)}
              />
              <div className={styles.groupCardBody}>
                <div className={styles.groupCardLabel}>{g.label}</div>
                <div className={styles.groupCardDesc}>{g.description}</div>
              </div>
            </label>
          ))}
        </div>
        {selected.has('audit_log') && (
          <div className={styles.warning}>
            ⚠ The audit log can be very large. Including it may significantly increase
            export size and time.
          </div>
        )}
        <div className={styles.info}>
          File attachments are not included in exports (model documents and incident
          attachments live on the server filesystem). Copy those files separately if you
          need them.
        </div>
      </section>

      {/* Section 2: Preview */}
      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Preview</h3>
        {noneChecked ? (
          <p className={styles.empty}>
            Select at least one entity group to preview the export.
          </p>
        ) : previewQuery.isLoading || (previewQuery.isFetching && !previewQuery.data) ? (
          <div className={styles.skeleton}>Loading preview…</div>
        ) : previewQuery.error ? (
          <p className={styles.errorText}>
            {(previewQuery.error as Error).message}
          </p>
        ) : (
          <>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>Table</th>
                  <th className={styles.numCol}>Row count</th>
                </tr>
              </thead>
              <tbody>
                {Object.entries(counts).map(([t, n]) => (
                  <tr key={t}>
                    <td>{t}</td>
                    <td className={styles.numCol}>{n}</td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th>Total</th>
                  <th className={styles.numCol}>{totalRows}</th>
                </tr>
              </tfoot>
            </table>
            <p className={styles.sizeNote}>
              Estimated size: {humaniseBytes(estimatedSize)}
            </p>
          </>
        )}
      </section>

      {/* Section 3: Export action */}
      <section className={styles.actionRow}>
        <button
          className={styles.btnPrimary}
          onClick={onExportClick}
          disabled={noneChecked || previewQuery.isLoading}
        >
          Export
        </button>
        {error && <span className={styles.errorText}>{error}</span>}
      </section>

      {/* Section 4: Recent exports */}
      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Recent exports</h3>
        {jobsQuery.isLoading ? (
          <div className={styles.skeleton}>Loading history…</div>
        ) : exportJobs.length === 0 ? (
          <p className={styles.empty}>No exports yet.</p>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th>When</th>
                <th>By</th>
                <th>Groups</th>
                <th>Status</th>
                <th className={styles.numCol}>Size</th>
              </tr>
            </thead>
            <tbody>
              {exportJobs.map((job) => {
                const groupsText = formatGroupsField(job.entity_groups);
                const sizeBytes =
                  (job.summary?.size_bytes as number | undefined) ??
                  (job.summary?.estimated_size_bytes as number | undefined);
                return (
                  <tr key={job.id}>
                    <td>{relativeTime(job.completed_at ?? job.started_at)}</td>
                    <td>{job.user_code}</td>
                    <td className={styles.groupsCell}>{groupsText}</td>
                    <td>
                      <span className={`${styles.badge} ${badgeClass(job.status)}`}>
                        {job.status}
                      </span>
                    </td>
                    <td className={styles.numCol}>
                      {sizeBytes ? humaniseBytes(sizeBytes) : '—'}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      <ExportSummaryModal
        isOpen={showModal}
        selectedGroups={Array.from(selected)}
        counts={counts}
        estimatedSizeBytes={estimatedSize}
        isSubmitting={exporting}
        onConfirm={onConfirmExport}
        onCancel={() => setShowModal(false)}
      />
    </div>
  );
}

function formatGroupsField(value: EntityGroup[] | string | null | undefined): string {
  if (!value) return '—';
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) {
    if (value.length === ENTITY_GROUPS.length) return 'all';
    return value.join(', ');
  }
  return '—';
}

function badgeClass(status: string): string {
  if (status === 'completed') return styles.badgeOk;
  if (status === 'failed') return styles.badgeCritical;
  if (status === 'running' || status === 'pending') return styles.badgeWarn;
  return '';
}
