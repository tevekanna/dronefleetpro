// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import type { ChangeEvent, FormEvent } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchBatteryTests, createBatteryTest } from '../../api/batteryTests';
import type {
  BatteryTest,
  BatteryTestResult,
  CreateBatteryTestRequest,
  SwellingStatus,
} from '../../types/batteryTest';
import { useCurrentUser } from '../../hooks/useCurrentUser';
import { userHasAnyRole } from '../../utils/auth';
import { IsoDateInput } from '../inputs/IsoDateInput';
import { formatISODate, todayISO } from '../../utils/date';
import { showToast } from '../../store/toastStore';
import shared from '../../pages/FleetStatsShared.module.css';
import styles from './BatteryTestsSection.module.css';

function resultBadgeClass(result: BatteryTestResult): string {
  if (result === 'pass') return styles.resultPass;
  if (result === 'warning') return styles.resultWarning;
  return styles.resultFail;
}

function parseCellVoltages(raw: string): number[] | undefined {
  const parsed = raw
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map((s) => Number(s))
    .filter((n) => !Number.isNaN(n));
  return parsed.length > 0 ? parsed : undefined;
}

function fadePercent(measured: number, nominal: number): string {
  return `${((1 - measured / nominal) * 100).toFixed(1)}%`;
}

interface AddTestModalProps {
  batteryId: number;
  onClose: () => void;
}

function AddTestModal({ batteryId, onClose }: AddTestModalProps) {
  const queryClient = useQueryClient();
  const [testDate, setTestDate] = useState(todayISO());
  const [capacity, setCapacity] = useState('');
  const [resistance, setResistance] = useState('');
  const [swelling, setSwelling] = useState<SwellingStatus>('none');
  const [cellVoltages, setCellVoltages] = useState('');
  const [result, setResult] = useState<BatteryTestResult>('pass');
  const [notes, setNotes] = useState('');

  const mutation = useMutation({
    mutationFn: (data: CreateBatteryTestRequest) => createBatteryTest(batteryId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['battery-tests', batteryId] });
      queryClient.invalidateQueries({ queryKey: ['fleet-stats', 'battery', batteryId] });
      showToast('Battery test logged', 'success');
      onClose();
    },
  });

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    const capacityNum = capacity.trim() === '' ? undefined : Number(capacity);
    const resistanceNum = resistance.trim() === '' ? undefined : Number(resistance);
    const data: CreateBatteryTestRequest = {
      test_date: testDate,
      result,
      swelling_status: swelling,
      capacity_measured:
        capacityNum !== undefined && !Number.isNaN(capacityNum) ? capacityNum : undefined,
      internal_resistance:
        resistanceNum !== undefined && !Number.isNaN(resistanceNum) ? resistanceNum : undefined,
      cell_voltages: parseCellVoltages(cellVoltages),
      notes: notes.trim() === '' ? undefined : notes.trim(),
    };
    mutation.mutate(data);
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.modalHeader}>
          <h3>Log battery test</h3>
          <button type="button" className={styles.closeBtn} onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <form onSubmit={handleSubmit}>
          <div className={styles.modalBody}>
            <div className={styles.form}>
              <div className={styles.formRow}>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="bt-date">
                    Test date
                  </label>
                  <IsoDateInput
                    id="bt-date"
                    className={styles.input}
                    value={testDate}
                    onChange={(e: ChangeEvent<HTMLInputElement>) => setTestDate(e.target.value)}
                    required
                  />
                </div>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="bt-result">
                    Result
                  </label>
                  <select
                    id="bt-result"
                    className={styles.select}
                    value={result}
                    onChange={(e) => setResult(e.target.value as BatteryTestResult)}
                    required
                  >
                    <option value="pass">Pass</option>
                    <option value="warning">Warning</option>
                    <option value="fail">Fail</option>
                  </select>
                </div>
              </div>

              <div className={styles.formRow}>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="bt-capacity">
                    Measured capacity (Wh)
                  </label>
                  <input
                    id="bt-capacity"
                    className={styles.input}
                    type="number"
                    step="any"
                    value={capacity}
                    onChange={(e) => setCapacity(e.target.value)}
                  />
                </div>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="bt-resistance">
                    Internal resistance (mΩ)
                  </label>
                  <input
                    id="bt-resistance"
                    className={styles.input}
                    type="number"
                    step="any"
                    value={resistance}
                    onChange={(e) => setResistance(e.target.value)}
                  />
                </div>
              </div>

              <div className={styles.formRow}>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="bt-swelling">
                    Swelling
                  </label>
                  <select
                    id="bt-swelling"
                    className={styles.select}
                    value={swelling}
                    onChange={(e) => setSwelling(e.target.value as SwellingStatus)}
                  >
                    <option value="none">None</option>
                    <option value="minor">Minor</option>
                    <option value="severe">Severe</option>
                  </select>
                </div>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="bt-cells">
                    Cell voltages
                  </label>
                  <input
                    id="bt-cells"
                    className={styles.input}
                    type="text"
                    placeholder="e.g. 4.18, 4.17, 4.19, 4.16"
                    value={cellVoltages}
                    onChange={(e) => setCellVoltages(e.target.value)}
                  />
                </div>
              </div>

              <div className={styles.field}>
                <label className={styles.label} htmlFor="bt-notes">
                  Notes
                </label>
                <textarea
                  id="bt-notes"
                  className={styles.textarea}
                  value={notes}
                  onChange={(e) => setNotes(e.target.value)}
                />
              </div>

              {mutation.isError && (
                <div className={styles.formError}>
                  {(mutation.error as Error).message}
                </div>
              )}
            </div>
          </div>
          <div className={styles.modalFooter}>
            <button type="button" className={styles.cancelBtn} onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className={styles.submitBtn} disabled={mutation.isPending}>
              {mutation.isPending ? 'Saving…' : 'Log test'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

interface BatteryTestsSectionProps {
  batteryId: number;
  nominalCapacityWh: number | null;
}

export function BatteryTestsSection({ batteryId, nominalCapacityWh }: BatteryTestsSectionProps) {
  const [showModal, setShowModal] = useState(false);
  const { data: currentUser } = useCurrentUser();
  const canEdit = userHasAnyRole(currentUser, 'admin', 'technician');
  const showFade = nominalCapacityWh != null;

  const { data, isLoading, error } = useQuery({
    queryKey: ['battery-tests', batteryId],
    queryFn: () => fetchBatteryTests(batteryId),
  });

  const tests: BatteryTest[] = data ?? [];

  return (
    <section className={shared.section}>
      <div className={styles.sectionHeader}>
        <h2 className={shared.sectionTitle}>Battery tests</h2>
        {canEdit && (
          <button type="button" className={styles.addBtn} onClick={() => setShowModal(true)}>
            Add test
          </button>
        )}
      </div>

      {isLoading ? (
        <p className={shared.loadingMsg}>Loading…</p>
      ) : error ? (
        <p className={shared.errorMsg}>Failed: {(error as Error).message}</p>
      ) : tests.length === 0 ? (
        <div className={shared.empty}>No battery tests logged yet.</div>
      ) : (
        <table className={shared.table}>
          <thead>
            <tr>
              <th className={shared.th}>Date</th>
              <th className={shared.th}>Result</th>
              <th className={shared.th}>Capacity (Wh)</th>
              {showFade && <th className={shared.th}>Fade</th>}
              <th className={shared.th}>Resistance (mΩ)</th>
              <th className={shared.th}>Swelling</th>
              <th className={shared.th}>Cell voltages</th>
              <th className={shared.th}>Tested by</th>
              <th className={shared.th}>Notes</th>
            </tr>
          </thead>
          <tbody>
            {tests.map((t) => (
              <tr key={t.id}>
                <td className={`${shared.td} ${shared.mono}`}>{formatISODate(t.test_date)}</td>
                <td className={shared.td}>
                  <span className={`${styles.resultBadge} ${resultBadgeClass(t.result)}`}>
                    {t.result}
                  </span>
                </td>
                <td className={shared.td}>
                  {t.capacity_measured != null ? t.capacity_measured : '—'}
                </td>
                {showFade && (
                  <td className={shared.td}>
                    {nominalCapacityWh != null && t.capacity_measured != null
                      ? fadePercent(t.capacity_measured, nominalCapacityWh)
                      : '—'}
                  </td>
                )}
                <td className={shared.td}>
                  {t.internal_resistance != null ? t.internal_resistance : '—'}
                </td>
                <td className={shared.td}>{t.swelling_status}</td>
                <td className={shared.td}>
                  {t.cell_voltages != null && t.cell_voltages.length > 0
                    ? t.cell_voltages.map((v) => `${v} V`).join(', ')
                    : '—'}
                </td>
                <td className={shared.td}>{t.tested_by ?? '—'}</td>
                <td className={shared.td}>{t.notes ?? '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {showModal && <AddTestModal batteryId={batteryId} onClose={() => setShowModal(false)} />}
    </section>
  );
}
