// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { applyImport, listRecentJobs, previewImport } from '../../api/dataTransfer';
import type {
  ConflictStrategy,
  EntityGroup,
  ImportSummary,
  PerTableCounts,
} from '../../types/dataTransfer';
import { humaniseBytes, relativeTime } from '../../utils/download';
import { describeDocument, parseImportFile, type ParsedDocument } from '../../utils/upload';
import { ImportConfirmModal } from './ImportConfirmModal';
import styles from './ImportTab.module.css';

const STRATEGY_OPTIONS: {
  value: ConflictStrategy;
  label: string;
  desc: string;
}[] = [
  {
    value: 'skip',
    label: 'Skip existing',
    desc: 'Keep existing records unchanged. Import only new ones.',
  },
  {
    value: 'update',
    label: 'Update existing',
    desc: 'Overwrite existing records with values from the file. Existing scrubbed-secret values are preserved.',
  },
  {
    value: 'fail',
    label: 'Fail on conflict',
    desc: 'Abort the entire import if any record already exists. Use for atomic imports into a clean target.',
  },
];

export function ImportTab() {
  const queryClient = useQueryClient();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [parsed, setParsed] = useState<ParsedDocument | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  const [parsing, setParsing] = useState(false);
  const [strategy, setStrategy] = useState<ConflictStrategy>('skip');
  // Default OFF: keep this instance's own logo unless the operator opts in.
  const [importBranding, setImportBranding] = useState(false);
  const [previewSummary, setPreviewSummary] = useState<ImportSummary | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const [isApplying, setIsApplying] = useState(false);
  const [applyResult, setApplyResult] = useState<ImportSummary | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [showAllConflicts, setShowAllConflicts] = useState(false);

  const jobsQuery = useQuery({
    queryKey: ['dataTransferJobs'],
    queryFn: () => listRecentJobs(20),
    staleTime: 10 * 1000,
  });

  const importJobs = (jobsQuery.data ?? []).filter((j) => j.job_type === 'import');

  // Groups to exclude from the import. Organisation branding is opt-in.
  const skipGroups: EntityGroup[] = importBranding ? [] : ['organisation'];

  const resetAll = () => {
    setParsed(null);
    setParseError(null);
    setStrategy('skip');
    setImportBranding(false);
    setPreviewSummary(null);
    setPreviewError(null);
    setApplyResult(null);
    setApplyError(null);
    setShowAllConflicts(false);
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  const handleFile = async (file: File) => {
    setParsing(true);
    setParseError(null);
    setParsed(null);
    setPreviewSummary(null);
    setApplyResult(null);
    setApplyError(null);
    try {
      const doc = await parseImportFile(file);
      setParsed(doc);
    } catch (e) {
      setParseError(e instanceof Error ? e.message : 'Failed to read file');
    } finally {
      setParsing(false);
    }
  };

  const onDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    const f = e.dataTransfer.files[0];
    if (f) void handleFile(f);
  };

  const onFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (f) void handleFile(f);
  };

  const onStrategyChange = (s: ConflictStrategy) => {
    setStrategy(s);
    // Stale results — clear so the user has to re-preview.
    setPreviewSummary(null);
    setApplyResult(null);
    setApplyError(null);
  };

  const onImportBrandingChange = (checked: boolean) => {
    setImportBranding(checked);
    // Stale results — the skip set changed, so re-preview is required.
    setPreviewSummary(null);
    setApplyResult(null);
    setApplyError(null);
  };

  const onPreview = async () => {
    if (!parsed) return;
    setIsPreviewing(true);
    setPreviewError(null);
    setApplyResult(null);
    setApplyError(null);
    try {
      const sum = await previewImport(parsed.json, strategy, skipGroups);
      setPreviewSummary(sum);
    } catch (e) {
      setPreviewError(e instanceof Error ? e.message : 'Preview failed');
    } finally {
      setIsPreviewing(false);
    }
  };

  const onApplyClick = () => {
    setApplyError(null);
    setShowConfirm(true);
  };

  const onConfirmApply = async () => {
    if (!parsed) return;
    setIsApplying(true);
    setApplyError(null);
    try {
      const sum = await applyImport(parsed.json, strategy, skipGroups);
      setApplyResult(sum);
      setShowConfirm(false);
      setPreviewSummary(null);
      queryClient.invalidateQueries({ queryKey: ['dataTransferJobs'] });
    } catch (e) {
      setApplyError(e instanceof Error ? e.message : 'Import failed');
      setShowConfirm(false);
    } finally {
      setIsApplying(false);
    }
  };

  const docInfo = parsed ? describeDocument(parsed.json) : null;

  return (
    <div className={styles.tab}>
      {/* Section 1: File picker */}
      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Choose an export file</h3>
        <div
          className={styles.dropZone}
          onDragOver={(e) => e.preventDefault()}
          onDrop={onDrop}
          onClick={() => fileInputRef.current?.click()}
        >
          <p className={styles.dropHeading}>
            {parsing
              ? 'Reading file…'
              : parsed
                ? 'Replace file: drop another or click to browse'
                : 'Drop .json or .zip here, or click to browse'}
          </p>
          <p className={styles.dropSub}>
            Accepts a JSON snapshot from this app, optionally inside a ZIP. Max 50 MB.
          </p>
        </div>
        <input
          ref={fileInputRef}
          type="file"
          accept=".json,.zip,application/json,application/zip"
          style={{ display: 'none' }}
          onChange={onFileSelect}
        />
        {parseError && <p className={styles.errorText}>{parseError}</p>}
        {parsed && docInfo && (
          <dl className={styles.fileMeta}>
            <div><dt>Filename</dt><dd>{parsed.filename}</dd></div>
            <div><dt>Size</dt><dd>{humaniseBytes(parsed.byteSize)}</dd></div>
            <div><dt>Format version</dt><dd>{docInfo.formatVersion ?? '—'}</dd></div>
            <div><dt>Source org id</dt><dd>{docInfo.sourceOrgId ?? '—'}</dd></div>
            <div><dt>Tables</dt><dd>{docInfo.tableCount}</dd></div>
            {parsed.wasZip && <div><dt>Type</dt><dd>extracted from ZIP</dd></div>}
          </dl>
        )}
      </section>

      {/* Section 2: Strategy */}
      {parsed && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>Conflict strategy</h3>
          <div className={styles.strategyList}>
            {STRATEGY_OPTIONS.map((opt) => (
              <label key={opt.value} className={styles.strategyRow}>
                <input
                  type="radio"
                  name="conflict-strategy"
                  value={opt.value}
                  checked={strategy === opt.value}
                  onChange={() => onStrategyChange(opt.value)}
                />
                <div className={styles.strategyBody}>
                  <div className={styles.strategyLabel}>{opt.label}</div>
                  <div className={styles.strategyDesc}>{opt.desc}</div>
                </div>
              </label>
            ))}
            <label className={styles.strategyRow} style={{ marginTop: 'var(--space-3)' }}>
              <input
                type="checkbox"
                checked={importBranding}
                onChange={(e) => onImportBrandingChange(e.target.checked)}
              />
              <div className={styles.strategyBody}>
                <div className={styles.strategyLabel}>Import organisation branding</div>
                <div className={styles.strategyDesc}>Off keeps this instance's own logo.</div>
              </div>
            </label>
          </div>
        </section>
      )}

      {/* Section 3: Preview action */}
      {parsed && !applyResult && (
        <section className={styles.actionRow}>
          <button
            className={styles.btnPrimary}
            onClick={onPreview}
            disabled={isPreviewing}
          >
            {isPreviewing ? 'Previewing…' : 'Preview import'}
          </button>
          {previewError && <span className={styles.errorText}>{previewError}</span>}
        </section>
      )}

      {/* Section 4: Preview results */}
      {previewSummary && !applyResult && (
        <ResultsCard
          title="Preview"
          summary={previewSummary}
          actionLabels={{
            created: 'Would create',
            updated: 'Would update',
            skipped: 'Would skip',
            failed: 'Would fail',
          }}
          showAllConflicts={showAllConflicts}
          onToggleConflicts={() => setShowAllConflicts((v) => !v)}
        />
      )}

      {/* Section 5: Apply button */}
      {previewSummary && !applyResult && (
        <section className={styles.actionRow}>
          <button className={styles.btnPrimary} onClick={onApplyClick}>
            Apply import
          </button>
          {applyError && <span className={styles.errorText}>{applyError}</span>}
        </section>
      )}

      {/* Section 6: Apply results */}
      {applyResult && (
        <>
          <ResultsCard
            title="Import complete"
            summary={applyResult}
            actionLabels={{
              created: 'Created',
              updated: 'Updated',
              skipped: 'Skipped',
              failed: 'Failed',
            }}
            showAllConflicts={showAllConflicts}
            onToggleConflicts={() => setShowAllConflicts((v) => !v)}
            isSuccess
          />
          <section className={styles.actionRow}>
            <button className={styles.btnSecondary} onClick={resetAll}>
              Import another file
            </button>
          </section>
        </>
      )}

      {/* Section 7: Recent imports */}
      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Recent imports</h3>
        {jobsQuery.isLoading ? (
          <div className={styles.skeleton}>Loading history…</div>
        ) : importJobs.length === 0 ? (
          <p className={styles.empty}>No imports yet.</p>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th>When</th>
                <th>By</th>
                <th>Status</th>
                <th className={styles.numCol}>Created</th>
                <th className={styles.numCol}>Skipped</th>
                <th>Notes</th>
              </tr>
            </thead>
            <tbody>
              {importJobs.map((job) => {
                const summary = job.summary as Record<string, unknown> | null;
                const created = summary?.total_created as number | undefined;
                const skipped = summary?.total_skipped as number | undefined;
                return (
                  <tr key={job.id}>
                    <td>{relativeTime(job.completed_at ?? job.started_at)}</td>
                    <td>{job.user_code}</td>
                    <td>
                      <span className={`${styles.badge} ${badgeClass(job.status)}`}>
                        {job.status}
                      </span>
                    </td>
                    <td className={styles.numCol}>{created ?? '—'}</td>
                    <td className={styles.numCol}>{skipped ?? '—'}</td>
                    <td className={styles.notesCell}>
                      {job.error_message ?? (summary?.dry_run ? 'preview only' : '')}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      <ImportConfirmModal
        isOpen={showConfirm}
        strategy={strategy}
        preview={previewSummary}
        isSubmitting={isApplying}
        onConfirm={onConfirmApply}
        onCancel={() => setShowConfirm(false)}
      />
    </div>
  );
}

interface ResultsCardProps {
  title: string;
  summary: ImportSummary;
  actionLabels: { created: string; updated: string; skipped: string; failed: string };
  showAllConflicts: boolean;
  onToggleConflicts: () => void;
  isSuccess?: boolean;
}

function ResultsCard({
  title,
  summary,
  actionLabels,
  showAllConflicts,
  onToggleConflicts,
  isSuccess,
}: ResultsCardProps) {
  const tableRows = Object.entries(summary.per_table)
    .filter(([, v]) => totalForRow(v) > 0)
    .sort();

  const visibleConflicts = showAllConflicts
    ? summary.conflicts
    : summary.conflicts.slice(0, 50);

  // Split the "failed" total into Path A (FK-fail: cannot import regardless of
  // strategy) and Path B (natural-key collision under Fail strategy: would
  // succeed under Skip/Update). Counts come from the conflict detail list, so
  // they may undercount when summary.conflicts_truncated — total_failed stays
  // authoritative for the combined number.
  const willFailCount = summary.conflicts.filter((c) => c.action === 'failed').length;
  const wouldFailCount = summary.conflicts.filter((c) => c.action === 'would_fail').length;

  return (
    <section className={`${styles.section} ${isSuccess ? styles.successBorder : ''}`}>
      <div className={styles.resultsHeader}>
        <h3 className={styles.sectionTitle} style={{ marginBottom: 0, paddingBottom: 0, border: 'none' }}>
          {title}
        </h3>
        {isSuccess && <span className={`${styles.badge} ${styles.badgeOk}`}>completed</span>}
        {summary.dry_run && <span className={`${styles.badge} ${styles.badgeWarn}`}>dry run</span>}
      </div>
      <div className={styles.totalsRow}>
        <Totals label={actionLabels.created} value={summary.total_created} tone="ok" />
        <Totals label={actionLabels.updated} value={summary.total_updated} tone="info" />
        <Totals label={actionLabels.skipped} value={summary.total_skipped} tone="muted" />
        {summary.dry_run ? (
          <>
            <Totals label="Will fail" value={willFailCount} tone="critical" />
            <Totals label="Would fail on Fail strategy" value={wouldFailCount} tone="critical" />
          </>
        ) : (
          <Totals label={actionLabels.failed} value={summary.total_failed} tone="critical" />
        )}
      </div>
      {tableRows.length > 0 ? (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Table</th>
              <th className={styles.numCol}>Created</th>
              <th className={styles.numCol}>Updated</th>
              <th className={styles.numCol}>Skipped</th>
              <th className={styles.numCol}>Failed</th>
            </tr>
          </thead>
          <tbody>
            {tableRows.map(([t, c]) => (
              <tr key={t}>
                <td>{t}</td>
                <td className={styles.numCol}>{c.created ?? 0}</td>
                <td className={styles.numCol}>{c.updated ?? 0}</td>
                <td className={styles.numCol}>{c.skipped ?? 0}</td>
                <td className={styles.numCol}>{c.failed ?? 0}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th>Total</th>
              <th className={styles.numCol}>{summary.total_created}</th>
              <th className={styles.numCol}>{summary.total_updated}</th>
              <th className={styles.numCol}>{summary.total_skipped}</th>
              <th className={styles.numCol}>{summary.total_failed}</th>
            </tr>
          </tfoot>
        </table>
      ) : (
        <p className={styles.empty}>No rows in any table — nothing to do.</p>
      )}

      {summary.conflicts.length > 0 && (
        <details className={styles.conflicts} open={summary.total_failed > 0}>
          <summary>Conflicts ({summary.conflicts.length}{summary.conflicts_truncated ? '+' : ''})</summary>
          <table className={styles.smallTable}>
            <thead>
              <tr>
                <th>Table</th>
                <th>Natural key</th>
                <th>Action</th>
                <th>Reason</th>
              </tr>
            </thead>
            <tbody>
              {visibleConflicts.map((c, i) => (
                <tr key={i}>
                  <td>{c.table}</td>
                  <td className={styles.keyCell}>
                    <code>{JSON.stringify(c.natural_key)}</code>
                  </td>
                  <td>{c.action}</td>
                  <td className={styles.reasonCell}>{c.reason ?? ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {summary.conflicts.length > 50 && (
            <button className={styles.btnLink} onClick={onToggleConflicts}>
              {showAllConflicts ? 'Show first 50' : `Show all ${summary.conflicts.length}`}
            </button>
          )}
          {summary.conflicts_truncated && (
            <p className={styles.note}>
              Showing first 100 conflicts. Apply to see how the full set is handled.
            </p>
          )}
        </details>
      )}
    </section>
  );
}

function Totals({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone: 'ok' | 'info' | 'muted' | 'critical';
}) {
  return (
    <div className={`${styles.totalsCell} ${styles['tone_' + tone]}`}>
      <div className={styles.totalsLabel}>{label}</div>
      <div className={styles.totalsValue}>{value}</div>
    </div>
  );
}

function totalForRow(c: PerTableCounts): number {
  return (c.created ?? 0) + (c.updated ?? 0) + (c.skipped ?? 0) + (c.failed ?? 0);
}

function badgeClass(status: string): string {
  if (status === 'completed') return styles.badgeOk;
  if (status === 'failed') return styles.badgeCritical;
  if (status === 'running' || status === 'pending') return styles.badgeWarn;
  return '';
}
