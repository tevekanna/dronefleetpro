// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  createAircraftModel,
  updateAircraftModel,
  type AircraftModel,
  type AircraftModelWithCount,
} from '../../api/aircraftModels';
import styles from '../users/UserFormModal.module.css';

interface Props {
  /** If provided, the modal acts as an edit form; otherwise create. */
  model?: AircraftModelWithCount | AircraftModel;
  onClose: () => void;
}

export function ModelFormModal({ model, onClose }: Props) {
  const queryClient = useQueryClient();
  const isEdit = !!model;

  const [name, setName] = useState(model?.name ?? '');
  const [manufacturer, setManufacturer] = useState(model?.manufacturer ?? '');
  const [category, setCategory] = useState(model?.category ?? '');
  const [notes, setNotes] = useState(model?.notes ?? '');
  const [maxFlight, setMaxFlight] = useState(
    model?.default_max_flight_time_minutes != null
      ? String(model.default_max_flight_time_minutes)
      : ''
  );
  const [weight, setWeight] = useState(
    model?.default_weight_kg != null ? String(model.default_weight_kg) : ''
  );
  const [mtom, setMtom] = useState(model?.mtom_kg != null ? String(model.mtom_kg) : '');
  const [emptyWeight, setEmptyWeight] = useState(
    model?.empty_weight_kg != null ? String(model.empty_weight_kg) : ''
  );
  const [maxPayload, setMaxPayload] = useState(
    model?.max_payload_kg != null ? String(model.max_payload_kg) : ''
  );
  const [classMarking, setClassMarking] = useState(model?.class_marking ?? '');
  const [maxDimension, setMaxDimension] = useState(
    model?.max_dimension_cm != null ? String(model.max_dimension_cm) : ''
  );
  const [designApproval, setDesignApproval] = useState(model?.design_approval_required ?? '');
  const [err, setErr] = useState<string | null>(null);

  const m = useMutation({
    mutationFn: async () => {
      const payload = {
        name: name.trim(),
        manufacturer: manufacturer.trim() || null,
        category: category.trim() || null,
        notes: notes.trim() || null,
        default_max_flight_time_minutes: maxFlight.trim() === '' ? null : Number(maxFlight),
        default_weight_kg: weight.trim() === '' ? null : Number(weight),
        mtom_kg: mtom.trim() === '' ? null : Number(mtom),
        empty_weight_kg: emptyWeight.trim() === '' ? null : Number(emptyWeight),
        max_payload_kg: maxPayload.trim() === '' ? null : Number(maxPayload),
        class_marking: classMarking === '' ? null : classMarking,
        max_dimension_cm: maxDimension.trim() === '' ? null : Number(maxDimension),
        // Explicit null = "Not determined" — tri-state on update, so the
        // stored assessment can be cleared, never silently kept.
        design_approval_required:
          designApproval === '' ? null : (designApproval as 'required' | 'not_required'),
      };
      if (isEdit && model) {
        return updateAircraftModel(model.id, payload);
      }
      return createAircraftModel(payload);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['aircraft-models'] });
      onClose();
    },
    onError: (e: Error) => setErr(e.message),
  });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    setErr(null);
    if (!name.trim()) { setErr('Name is required'); return; }
    if (maxFlight && isNaN(Number(maxFlight))) { setErr('Max flight time must be a number'); return; }
    if (weight && isNaN(Number(weight))) { setErr('Weight must be a number'); return; }
    // Each regulatory measurement, when entered, must be a positive number.
    const positiveFields: Array<[string, string]> = [
      ['MTOM', mtom],
      ['Empty weight', emptyWeight],
      ['Max payload', maxPayload],
      ['Max dimension', maxDimension],
    ];
    for (const [label, val] of positiveFields) {
      if (val.trim() === '') continue;
      const n = Number(val);
      if (isNaN(n)) { setErr(`${label} must be a number`); return; }
      if (n <= 0) { setErr(`${label} must be greater than 0`); return; }
    }
    if (mtom.trim() !== '' && emptyWeight.trim() !== '' && Number(mtom) < Number(emptyWeight)) {
      setErr('MTOM must be greater than or equal to empty weight'); return;
    }
    m.mutate();
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={`${styles.modal} ${styles.compact}`} onClick={e => e.stopPropagation()}>
        <div className={styles.title}>{isEdit ? 'Edit Model' : 'Add Model'}</div>
        <form onSubmit={submit}>
          <div className={styles.field}>
            <label className={styles.label}>Name *</label>
            <input className={styles.input} value={name} onChange={e => setName(e.target.value)} placeholder="e.g. DJI Mavic 3" />
          </div>
          <div className={styles.fieldGrid2col}>
            <div className={styles.field}>
              <label className={styles.label}>Manufacturer</label>
              <input className={styles.input} value={manufacturer} onChange={e => setManufacturer(e.target.value)} placeholder="e.g. DJI" />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Category</label>
              <select className={styles.select} value={category} onChange={e => setCategory(e.target.value)}>
                <option value="">—</option>
                <option value="consumer">consumer</option>
                <option value="enterprise">enterprise</option>
                <option value="custom">custom</option>
              </select>
              <div className={styles.helperText}>Free-text by convention (consumer / enterprise / custom).</div>
            </div>
          </div>
          <div className={styles.fieldGrid2col}>
            <div className={styles.field}>
              <label className={styles.label}>Default max flight time (minutes)</label>
              <input className={styles.input} value={maxFlight} onChange={e => setMaxFlight(e.target.value)} inputMode="numeric" placeholder="e.g. 30" />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Default weight (kg)</label>
              <input className={styles.input} value={weight} onChange={e => setWeight(e.target.value)} inputMode="decimal" placeholder="e.g. 0.9" />
              <div className={styles.helperText}>Legacy field, kept for compatibility. Use MTOM and empty weight below for regulatory data.</div>
            </div>
          </div>

          <div className={styles.groupHeading}>Weights &amp; EASA class</div>
          <div className={styles.fieldGrid2col}>
            <div className={styles.field}>
              <label className={styles.label}>MTOM (kg)</label>
              <input className={styles.input} value={mtom} onChange={e => setMtom(e.target.value)} inputMode="decimal" placeholder="e.g. 0.895" />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Empty weight (kg)</label>
              <input className={styles.input} value={emptyWeight} onChange={e => setEmptyWeight(e.target.value)} inputMode="decimal" placeholder="e.g. 0.63" />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Max payload (kg)</label>
              <input className={styles.input} value={maxPayload} onChange={e => setMaxPayload(e.target.value)} inputMode="decimal" placeholder="e.g. 0.25" />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Max dimension (cm)</label>
              <input className={styles.input} value={maxDimension} onChange={e => setMaxDimension(e.target.value)} inputMode="decimal" placeholder="e.g. 38" />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>EASA class marking</label>
              <select className={styles.select} value={classMarking} onChange={e => setClassMarking(e.target.value)}>
                <option value="">— Not marked —</option>
                <option value="C0">C0</option>
                <option value="C1">C1</option>
                <option value="C2">C2</option>
                <option value="C3">C3</option>
                <option value="C4">C4</option>
                <option value="C5">C5</option>
                <option value="C6">C6</option>
              </select>
              <div className={styles.helperText}>MTOM sets the maximum take-off mass ceiling. Many older airframes carry no class marking — leave as “Not marked”.</div>
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Design approval</label>
              <select className={styles.select} value={designApproval} onChange={e => setDesignApproval(e.target.value as '' | 'required' | 'not_required')}>
                <option value="">Not determined</option>
                <option value="required">Required</option>
                <option value="not_required">Not required</option>
              </select>
              <div className={styles.helperText}>
                Records whether the design requires a certificate or declaration under Article 56(1)
                and (5) of Regulation (EU) 2018/1139 — this is what makes Annex VI occurrence
                reporting mandatory. “Not determined” is not the same as “Not required”: leave it
                unset until the assessment has actually been made.
              </div>
            </div>
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Notes</label>
            <textarea
              className={styles.input}
              value={notes}
              onChange={e => setNotes(e.target.value)}
              rows={2}
              placeholder="e.g. Used for survey missions; 4K camera; …"
            />
          </div>
          {err && <div className={styles.formError}>{err}</div>}
          <div className={styles.actions}>
            <button type="button" className={styles.btnCancel} onClick={onClose}>Cancel</button>
            <button type="submit" className={styles.btnSubmit} disabled={m.isPending}>
              {m.isPending ? 'Saving…' : isEdit ? 'Save changes' : 'Add Model'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
