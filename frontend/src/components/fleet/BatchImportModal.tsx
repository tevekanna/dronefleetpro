// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useRef, useCallback, useEffect } from 'react';
import { Upload, CheckCircle2 } from 'lucide-react';
import styles from './BatchImportModal.module.css';
import type { BatchImportPreview, BatchImportRow, BatchCommitRow } from '../../api/batchImport';
import { previewBatchImport, commitBatchImport } from '../../api/batchImport';

type EntityType = 'aircraft' | 'battery' | 'controller';
type Step = 1 | 2 | 3;

interface BatchImportModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
  defaultEntityType?: EntityType;
}

const ENTITY_LABELS: Record<EntityType, string> = {
  aircraft: 'Aircraft',
  battery: 'Batteries',
  controller: 'Controllers',
};

export function BatchImportModal({
  isOpen,
  onClose,
  onSuccess,
  defaultEntityType = 'aircraft',
}: BatchImportModalProps) {
  const [step, setStep] = useState<Step>(1);
  const [entityType, setEntityType] = useState<EntityType>(defaultEntityType);
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<BatchImportPreview | null>(null);
  const [rowModels, setRowModels] = useState<Record<number, string>>({});
  const [rowManufacturers, setRowManufacturers] = useState<Record<number, string>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [result, setResult] = useState<{ created: number; skipped: number } | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isOpen) {
      setStep(1);
      setEntityType(defaultEntityType);
      setFile(null);
      setPreview(null);
      setRowModels({});
      setRowManufacturers({});
      setLoading(false);
      setError('');
      setResult(null);
      setDragOver(false);
    }
  }, [isOpen, defaultEntityType]);

  if (!isOpen) return null;

  const handleFileDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    const dropped = e.dataTransfer.files[0];
    if (dropped && dropped.name.endsWith('.csv')) {
      setFile(dropped);
      setError('');
    } else {
      setError('Please drop a .csv file');
    }
  };

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selected = e.target.files?.[0];
    if (selected) {
      setFile(selected);
      setError('');
    }
  };

  const handleParse = async () => {
    if (!file) return;
    setLoading(true);
    setError('');
    try {
      const data = await previewBatchImport(file, entityType);
      setPreview(data);
      // Pre-populate the preview inputs from any pass-through values:
      //  - battery's compatible-model guess (DJI prefix mapping)
      //  - Standard CSV's `model` column
      //  - Standard CSV's `manufacturer` column
      const models: Record<number, string> = {};
      const manufacturers: Record<number, string> = {};
      data.rows.forEach((row, idx) => {
        if (row.suggested_compatible_model) {
          models[idx] = row.suggested_compatible_model;
        }
        if (row.model) {
          models[idx] = row.model;
        }
        if (row.manufacturer) {
          manufacturers[idx] = row.manufacturer;
        }
      });
      setRowModels(models);
      setRowManufacturers(manufacturers);
      setStep(2);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Upload failed');
    } finally {
      setLoading(false);
    }
  };

  const handleCommit = async () => {
    if (!preview) return;
    setLoading(true);
    setError('');
    try {
      const commitRows: BatchCommitRow[] = preview.rows
        .filter((r) => r.status === 'new')
        .map((r) => {
          const idx = preview.rows.indexOf(r);
          return {
            serial_number: r.serial_number,
            compatible_model:
              entityType === 'battery' ? rowModels[idx] || null : undefined,
            model: entityType !== 'battery' ? rowModels[idx] || null : undefined,
            manufacturer:
              entityType === 'aircraft' ? rowManufacturers[idx] || null : undefined,
            max_cycles: entityType === 'battery' ? 300 : undefined,
            // Round-trip aircraft-only fields straight back. Other
            // entity types ignore them server-side.
            device_metadata: entityType === 'aircraft' ? r.device_metadata : undefined,
            needs_manual_sn: entityType === 'aircraft' ? r.needs_manual_sn : undefined,
          };
        });

      const res = await commitBatchImport({ entity_type: entityType, rows: commitRows });
      setResult({ created: res.created, skipped: res.skipped_duplicates });
      setStep(3);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Import failed');
    } finally {
      setLoading(false);
    }
  };

  const handleDone = () => {
    onSuccess();
    onClose();
  };

  const newCount = preview?.rows.filter((r) => r.status === 'new').length ?? 0;
  const dupCount = preview?.rows.filter((r) => r.status === 'duplicate').length ?? 0;

  const modelColumnLabel = entityType === 'battery' ? 'Compatible Model' : 'Model';
  const showManufacturer = entityType === 'aircraft';

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>Batch Import Fleet Equipment</h2>

        <StepIndicator current={step} />

        {step === 1 && (
          <StepUpload
            entityType={entityType}
            setEntityType={setEntityType}
            file={file}
            dragOver={dragOver}
            setDragOver={setDragOver}
            onFileDrop={handleFileDrop}
            onFileSelect={handleFileSelect}
            fileInputRef={fileInputRef}
            loading={loading}
            error={error}
            onParse={handleParse}
            onClose={onClose}
          />
        )}

        {step === 2 && preview && (
          <StepPreview
            preview={preview}
            modelColumnLabel={modelColumnLabel}
            showManufacturer={showManufacturer}
            rowModels={rowModels}
            setRowModels={setRowModels}
            rowManufacturers={rowManufacturers}
            setRowManufacturers={setRowManufacturers}
            newCount={newCount}
            dupCount={dupCount}
            loading={loading}
            error={error}
            onBack={() => setStep(1)}
            onCommit={handleCommit}
          />
        )}

        {step === 3 && result && (
          <StepResult
            entityType={entityType}
            created={result.created}
            skipped={result.skipped}
            onDone={handleDone}
          />
        )}
      </div>
    </div>
  );
}

function StepIndicator({ current }: { current: Step }) {
  const stepClass = (s: number) => {
    if (s < current) return `${styles.stepNumber} ${styles.completed}`;
    if (s === current) return `${styles.stepNumber} ${styles.active}`;
    return styles.stepNumber;
  };
  const labelClass = (s: number) =>
    s === current ? `${styles.stepLabel} ${styles.active}` : styles.stepLabel;
  const lineClass = (s: number) =>
    s < current ? `${styles.stepLine} ${styles.completed}` : styles.stepLine;

  return (
    <div className={styles.stepIndicator}>
      <div className={styles.step}>
        <span className={stepClass(1)}>1</span>
        <span className={labelClass(1)}>Upload</span>
      </div>
      <div className={lineClass(1)} />
      <div className={styles.step}>
        <span className={stepClass(2)}>2</span>
        <span className={labelClass(2)}>Preview</span>
      </div>
      <div className={lineClass(2)} />
      <div className={styles.step}>
        <span className={stepClass(3)}>3</span>
        <span className={labelClass(3)}>Done</span>
      </div>
    </div>
  );
}

interface StepUploadProps {
  entityType: EntityType;
  setEntityType: (t: EntityType) => void;
  file: File | null;
  dragOver: boolean;
  setDragOver: (v: boolean) => void;
  onFileDrop: (e: React.DragEvent) => void;
  onFileSelect: (e: React.ChangeEvent<HTMLInputElement>) => void;
  fileInputRef: React.RefObject<HTMLInputElement>;
  loading: boolean;
  error: string;
  onParse: () => void;
  onClose: () => void;
}

function StepUpload({
  entityType,
  setEntityType,
  file,
  dragOver,
  setDragOver,
  onFileDrop,
  onFileSelect,
  fileInputRef,
  loading,
  error,
  onParse,
  onClose,
}: StepUploadProps) {
  return (
    <>
      <div className={styles.entityTypeSelector}>
        {(['aircraft', 'battery', 'controller'] as EntityType[]).map((t) => (
          <button
            key={t}
            className={`${styles.entityTypeBtn} ${
              entityType === t ? styles.active : ''
            }`}
            onClick={() => setEntityType(t)}
          >
            {ENTITY_LABELS[t]}
          </button>
        ))}
      </div>

      <div
        className={`${styles.dropZone} ${dragOver ? styles.dragOver : ''}`}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onFileDrop}
        onClick={() => fileInputRef.current?.click()}
      >
        <Upload size={32} className={styles.dropZoneIcon} />
        <p className={styles.dropZoneText}>Drop a CSV file here or click to browse</p>
        {file && <p className={styles.fileName}>{file.name}</p>}
        <input
          ref={fileInputRef}
          type="file"
          accept=".csv"
          onChange={onFileSelect}
          className={styles.fileInput}
        />
      </div>

      <p className={styles.hint}>
        Upload a CSV file with serial numbers. Supported formats: Binary Eye
        (Android QR/DataMatrix scanner), DJI Mimo QR scanner, Tello QR scanner,
        or a plain CSV with a &quot;serial_number&quot; column. Within-file
        duplicates and obviously-invalid rows are dropped automatically.
      </p>

      {error && <p className={styles.error}>{error}</p>}

      <div className={styles.actions}>
        <button className={styles.btnSecondary} onClick={onClose}>
          Cancel
        </button>
        <button
          className={styles.btnPrimary}
          disabled={!file || loading}
          onClick={onParse}
        >
          {loading ? 'Parsing…' : 'Parse File'}
        </button>
      </div>
    </>
  );
}

interface StepPreviewProps {
  preview: BatchImportPreview;
  modelColumnLabel: string;
  showManufacturer: boolean;
  rowModels: Record<number, string>;
  setRowModels: React.Dispatch<React.SetStateAction<Record<number, string>>>;
  rowManufacturers: Record<number, string>;
  setRowManufacturers: React.Dispatch<React.SetStateAction<Record<number, string>>>;
  newCount: number;
  dupCount: number;
  loading: boolean;
  error: string;
  onBack: () => void;
  onCommit: () => void;
}

function StepPreview({
  preview,
  modelColumnLabel,
  showManufacturer,
  rowModels,
  setRowModels,
  rowManufacturers,
  setRowManufacturers,
  newCount,
  dupCount,
  loading,
  error,
  onBack,
  onCommit,
}: StepPreviewProps) {
  const [applyAllModel, setApplyAllModel] = useState('');
  const [applyAllManufacturer, setApplyAllManufacturer] = useState('');

  const applyAll = (
    value: string,
    setter: React.Dispatch<React.SetStateAction<Record<number, string>>>,
  ) => {
    if (!value.trim()) return;
    setter((prev) => {
      const next = { ...prev };
      preview.rows.forEach((row, idx) => {
        if (row.status === 'new' && !next[idx]) {
          next[idx] = value.trim();
        }
      });
      return next;
    });
  };

  const updateModel = useCallback(
    (idx: number, value: string) => {
      setRowModels((prev) => ({ ...prev, [idx]: value }));
    },
    [setRowModels],
  );

  const updateManufacturer = useCallback(
    (idx: number, value: string) => {
      setRowManufacturers((prev) => ({ ...prev, [idx]: value }));
    },
    [setRowManufacturers],
  );

  return (
    <>
      <PreviewSummary preview={preview} newCount={newCount} dupCount={dupCount} />

      {showManufacturer && (
        <div className={styles.applyAllRow}>
          <label>Manufacturer:</label>
          <input
            className={`${styles.modelInput} ${styles.applyAllInput}`}
            value={applyAllManufacturer}
            onChange={(e) => setApplyAllManufacturer(e.target.value)}
            placeholder="Set manufacturer for all empty rows"
          />
          <button
            className={styles.applyAllBtn}
            onClick={() => applyAll(applyAllManufacturer, setRowManufacturers)}
          >
            Apply to all
          </button>
        </div>
      )}

      <div className={styles.applyAllRow}>
        <label>{modelColumnLabel}:</label>
        <input
          className={`${styles.modelInput} ${styles.applyAllInput}`}
          value={applyAllModel}
          onChange={(e) => setApplyAllModel(e.target.value)}
          placeholder={`Set ${modelColumnLabel.toLowerCase()} for all empty rows`}
        />
        <button
          className={styles.applyAllBtn}
          onClick={() => applyAll(applyAllModel, setRowModels)}
        >
          Apply to all
        </button>
      </div>

      <div className={styles.scrollContainer}>
        <table className={styles.previewTable}>
          <thead>
            <tr>
              <th>Serial Number</th>
              <th>Status</th>
              {showManufacturer && <th>Manufacturer</th>}
              <th>{modelColumnLabel}</th>
            </tr>
          </thead>
          <tbody>
            {preview.rows.map((row, idx) => (
              <PreviewRow
                key={idx}
                row={row}
                idx={idx}
                showManufacturer={showManufacturer}
                modelValue={rowModels[idx] ?? ''}
                manufacturerValue={rowManufacturers[idx] ?? ''}
                onModelChange={updateModel}
                onManufacturerChange={updateManufacturer}
                isDuplicate={row.status === 'duplicate'}
              />
            ))}
          </tbody>
        </table>
      </div>

      {error && <p className={styles.error}>{error}</p>}

      <div className={styles.actions}>
        <button className={styles.btnSecondary} onClick={onBack}>
          Back
        </button>
        <button
          className={styles.btnPrimary}
          disabled={newCount === 0 || loading}
          onClick={onCommit}
        >
          {loading ? 'Importing…' : `Import ${newCount} items`}
        </button>
      </div>
    </>
  );
}

interface PreviewRowProps {
  row: BatchImportRow;
  idx: number;
  showManufacturer: boolean;
  modelValue: string;
  manufacturerValue: string;
  onModelChange: (idx: number, value: string) => void;
  onManufacturerChange: (idx: number, value: string) => void;
  isDuplicate: boolean;
}

function PreviewRow({
  row,
  idx,
  showManufacturer,
  modelValue,
  manufacturerValue,
  onModelChange,
  onManufacturerChange,
  isDuplicate,
}: PreviewRowProps) {
  const badgeClass =
    row.status === 'new'
      ? styles.statusBadgeNew
      : row.status === 'duplicate'
      ? styles.statusBadgeDuplicate
      : styles.statusBadgeInvalid;

  const statusLabel =
    row.status === 'new'
      ? 'New'
      : row.status === 'duplicate'
      ? 'Already exists'
      : 'Invalid';

  const dimmed = isDuplicate || row.status === 'invalid';

  return (
    <tr className={isDuplicate ? styles.duplicate : undefined}>
      <td className={styles.serialCell}>{row.serial_number}</td>
      <td>
        <span className={`${styles.statusBadge} ${badgeClass}`}>
          <span className={styles.statusDot} />
          {statusLabel}
        </span>
      </td>
      {showManufacturer && (
        <td>
          {dimmed ? (
            <span className={styles.muted}>—</span>
          ) : (
            <input
              className={styles.modelInput}
              value={manufacturerValue}
              onChange={(e) => onManufacturerChange(idx, e.target.value)}
              placeholder="e.g. DJI"
            />
          )}
        </td>
      )}
      <td>
        {dimmed ? (
          <span className={styles.muted}>—</span>
        ) : (
          <input
            className={styles.modelInput}
            value={modelValue}
            onChange={(e) => onModelChange(idx, e.target.value)}
            placeholder="Enter model"
          />
        )}
      </td>
    </tr>
  );
}

interface PreviewSummaryProps {
  preview: BatchImportPreview;
  newCount: number;
  dupCount: number;
}

function PreviewSummary({ preview, newCount, dupCount }: PreviewSummaryProps) {
  const [errorsExpanded, setErrorsExpanded] = useState(false);
  const formatLabel = preview.source_format_label ?? 'Unknown format';
  const errorCount = preview.errors.length;
  const warningCount = preview.warnings.length;

  return (
    <div className={styles.previewSummaryBlock}>
      <div className={styles.formatBanner}>
        <span className={styles.formatBannerLabel}>Detected format</span>
        <span className={styles.formatBannerValue}>{formatLabel}</span>
      </div>

      <div className={styles.summaryGrid}>
        <SummaryStat label="Rows scanned" value={preview.total_rows} />
        <SummaryStat label="Valid SNs" value={preview.total_parsed} highlight />
        <SummaryStat label="Duplicates removed" value={preview.duplicates_removed} />
        <SummaryStat label="Skipped (errors)" value={errorCount} />
      </div>

      <p className={styles.summary}>
        Of {preview.total_parsed} valid SNs:{' '}
        <span className={styles.summaryCount}>{newCount}</span> new,{' '}
        <span className={styles.summaryCount}>{dupCount}</span> already in system
      </p>

      {warningCount > 0 && (
        <div className={styles.warningPanel}>
          {preview.warnings.map((w, i) => (
            <p key={i} className={styles.warningLine}>{w}</p>
          ))}
        </div>
      )}

      {errorCount > 0 && (
        <div className={styles.errorPanel}>
          <button
            type="button"
            className={styles.errorPanelToggle}
            onClick={() => setErrorsExpanded((v) => !v)}
          >
            {errorsExpanded ? '▾' : '▸'} {errorCount} row
            {errorCount === 1 ? '' : 's'} skipped — show reasons
          </button>
          {errorsExpanded && (
            <ul className={styles.errorList}>
              {preview.errors.map((e, i) => (
                <li key={i} className={styles.errorListItem}>{e}</li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function SummaryStat({
  label,
  value,
  highlight = false,
}: {
  label: string;
  value: number;
  highlight?: boolean;
}) {
  return (
    <div className={styles.summaryStat}>
      <span className={styles.summaryStatLabel}>{label}</span>
      <span
        className={
          highlight
            ? `${styles.summaryStatValue} ${styles.summaryStatValueHighlight}`
            : styles.summaryStatValue
        }
      >
        {value}
      </span>
    </div>
  );
}

function StepResult({
  entityType,
  created,
  skipped,
  onDone,
}: {
  entityType: EntityType;
  created: number;
  skipped: number;
  onDone: () => void;
}) {
  return (
    <div className={styles.resultSummary}>
      <CheckCircle2 size={48} className={styles.resultIcon} />
      <p className={styles.resultTitle}>
        Successfully imported {created} {ENTITY_LABELS[entityType].toLowerCase()}
      </p>
      {skipped > 0 && (
        <p className={styles.resultSubtitle}>
          {skipped} serial number{skipped !== 1 ? 's were' : ' was'} skipped (already
          existed)
        </p>
      )}
      <div className={`${styles.actions} ${styles.actionsCenter}`}>
        <button className={styles.btnPrimary} onClick={onDone}>
          Done
        </button>
      </div>
    </div>
  );
}
