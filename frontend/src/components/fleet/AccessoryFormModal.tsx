// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createAccessory, updateAccessory } from '../../api/accessories';
import type { Accessory, CreateAccessoryRequest } from '../../types/accessory';
import type { Aircraft } from '../../types/aircraft';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import styles from '../users/UserFormModal.module.css';

/**
 * Create/edit modal for a weighted accessory. Mirrors the battery modal logic
 * pattern (useMutation → invalidate → close) and adopts the shared compact
 * two-column form layout from UserFormModal.module.css. Passing `accessory`
 * puts the modal in edit mode; omitting it is create mode.
 */
export function AccessoryFormModal({
  accessory,
  aircraft,
  onClose,
}: {
  accessory?: Accessory | null;
  aircraft: Aircraft[];
  onClose: () => void;
}) {
  const isEdit = accessory != null;
  const queryClient = useQueryClient();

  const [name, setName] = useState(accessory?.name ?? '');
  const [aircraftId, setAircraftId] = useState(accessory?.aircraft_id ? String(accessory.aircraft_id) : '');
  const [category, setCategory] = useState(accessory?.category ?? '');
  const [manufacturer, setManufacturer] = useState(accessory?.manufacturer ?? '');
  const [serialNumber, setSerialNumber] = useState(accessory?.serial_number ?? '');
  const [equipmentId, setEquipmentId] = useState(accessory?.equipment_id ?? '');
  const [weightKg, setWeightKg] = useState(
    accessory?.weight_kg != null ? String(accessory.weight_kg) : ''
  );
  const [status, setStatus] = useState(accessory?.status ?? 'active');
  const [notes, setNotes] = useState(accessory?.retirement_notes ?? '');
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: (data: Record<string, unknown>) =>
      isEdit
        ? updateAccessory(accessory!.id, data)
        : createAccessory(data as unknown as CreateAccessoryRequest),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['accessories'] });
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    if (!name.trim()) {
      setError('Name is required.');
      return;
    }
    const trimmedWeight = weightKg.trim();
    if (trimmedWeight !== '') {
      const w = Number(trimmedWeight);
      if (isNaN(w) || w <= 0) {
        setError('Weight must be a number greater than 0.');
        return;
      }
    }

    if (isEdit) {
      // COALESCE partial update: empty text fields → null (clears the value),
      // empty weight → undefined (omitted, keeps existing via COALESCE).
      mutation.mutate({
        name: name.trim(),
        aircraft_id: aircraftId ? Number(aircraftId) : null,
        category: category.trim() || null,
        manufacturer: manufacturer.trim() || null,
        serial_number: serialNumber.trim() || null,
        equipment_id: equipmentId.trim() || null,
        weight_kg: trimmedWeight === '' ? undefined : Number(trimmedWeight),
        status,
        retirement_notes: notes.trim() || null,
      });
    } else {
      mutation.mutate({
        name: name.trim(),
        aircraft_id: aircraftId ? Number(aircraftId) : undefined,
        category: category.trim() || undefined,
        manufacturer: manufacturer.trim() || undefined,
        serial_number: serialNumber.trim() || undefined,
        equipment_id: equipmentId.trim() || undefined,
        weight_kg: trimmedWeight === '' ? undefined : Number(trimmedWeight),
      });
    }
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={`${styles.modal} ${styles.compact}`}
        onClick={e => e.stopPropagation()}
      >
        <h2 className={styles.title}>{isEdit ? 'Edit Accessory' : 'Add Accessory'}</h2>
        <form onSubmit={handleSubmit}>
          <div className={styles.field}>
            <label className={styles.label}>Name *</label>
            <input
              className={styles.input}
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="e.g. Zenmuse H20T"
            />
          </div>
          <div className={styles.field}>
            <label className={styles.label}>Linked Aircraft</label>
            <select
              className={styles.select}
              value={aircraftId}
              onChange={e => setAircraftId(e.target.value)}
            >
              <option value="">— None —</option>
              {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
            </select>
          </div>
          <div className={styles.fieldGrid2col}>
            <div className={styles.field}>
              <label className={styles.label}>Category</label>
              <input
                className={styles.input}
                value={category}
                onChange={e => setCategory(e.target.value)}
                placeholder="e.g. camera, gimbal, payload"
              />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Manufacturer</label>
              <input
                className={styles.input}
                value={manufacturer}
                onChange={e => setManufacturer(e.target.value)}
                placeholder="e.g. DJI"
              />
            </div>
          </div>
          <div className={styles.fieldGrid2col}>
            <div className={styles.field}>
              <label className={styles.label}>Serial Number</label>
              <input
                className={styles.input}
                value={serialNumber}
                onChange={e => setSerialNumber(e.target.value)}
                placeholder="Optional"
              />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Equipment ID</label>
              <input
                className={styles.input}
                value={equipmentId}
                onChange={e => setEquipmentId(e.target.value)}
                placeholder="e.g. ACC-001"
              />
            </div>
          </div>
          <div className={styles.fieldGrid2col}>
            <div className={styles.field}>
              <label className={styles.label}>Weight (kg)</label>
              <input
                className={styles.input}
                value={weightKg}
                onChange={e => setWeightKg(e.target.value)}
                inputMode="decimal"
                placeholder="Measured mass, e.g. 0.83"
              />
            </div>
            {isEdit && (
              <div className={styles.field}>
                <label className={styles.label}>Status</label>
                <select
                  className={styles.select}
                  value={status}
                  onChange={e => setStatus(e.target.value as Accessory['status'])}
                >
                  <option value="active">Active</option>
                  <option value="maintenance">Maintenance</option>
                  <option value="retired">Retired</option>
                </select>
              </div>
            )}
          </div>
          {isEdit && (
            <div className={styles.field}>
              <label className={styles.label}>Notes</label>
              <textarea
                className={styles.input}
                style={{ minHeight: '50px', resize: 'vertical' }}
                value={notes}
                onChange={e => setNotes(e.target.value)}
                placeholder="Optional notes (e.g. retirement reason)…"
              />
            </div>
          )}
          {error && <p className={styles.formError}>{error}</p>}
          <div className={styles.actions}>
            <button type="button" onClick={onClose} className={styles.btnCancel}>
              Cancel
            </button>
            <button type="submit" disabled={mutation.isPending} className={styles.btnSubmit}>
              {mutation.isPending
                ? isEdit ? 'Saving…' : 'Adding…'
                : isEdit ? 'Save Changes' : 'Add Accessory'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
