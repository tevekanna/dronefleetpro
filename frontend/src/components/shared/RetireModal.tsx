// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import styles from './RetireModal.module.css';

interface RetireModalProps {
  isOpen: boolean;
  entityType: 'aircraft' | 'battery' | 'controller' | 'accessory';
  entityLabel: string;
  onConfirm: (notes: string) => void;
  onCancel: () => void;
  isSubmitting: boolean;
}

const ENTITY_NAMES: Record<string, string> = {
  aircraft: 'Aircraft',
  battery: 'Battery',
  controller: 'Controller',
  accessory: 'Accessory',
};

export function RetireModal({ isOpen, entityType, entityLabel, onConfirm, onCancel, isSubmitting }: RetireModalProps) {
  const [notes, setNotes] = useState('');

  if (!isOpen) return null;

  const handleConfirm = () => {
    onConfirm(notes.trim());
  };

  return (
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <h2 className={styles.title}>Retire {ENTITY_NAMES[entityType]}</h2>
        <p className={styles.body}>
          This will mark <strong>{entityLabel}</strong> as retired. It will no longer appear in
          active fleet lists or flight logging. This action can be reversed by editing the
          equipment and setting its status back to active.
        </p>
        <div className={styles.field}>
          <label className={styles.label}>Retirement notes (optional)</label>
          <textarea
            className={styles.textarea}
            value={notes}
            onChange={e => setNotes(e.target.value)}
            placeholder="Reason for retirement, e.g. beyond repair, sold, end of service life"
          />
        </div>
        <div className={styles.actions}>
          <button className={styles.btnCancel} onClick={onCancel}>Cancel</button>
          <button
            className={styles.btnRetire}
            onClick={handleConfirm}
            disabled={isSubmitting}
          >
            {isSubmitting ? 'Retiring...' : 'Retire'}
          </button>
        </div>
      </div>
    </div>
  );
}
