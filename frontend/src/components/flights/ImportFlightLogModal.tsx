// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchAircraft } from '../../api/aircraft';
import { fetchPilots } from '../../api/pilots';
import type { Aircraft } from '../../types/aircraft';
import type { Pilot } from '../../api/pilots';
import { FlightMap } from '../map/FlightMap';
import { formatDateTime } from '../flight-detail/flightFormatters';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import { showToast } from '../../store/toastStore';
import { ReportIncidentModal } from '../incidents/ReportIncidentModal';
import {
  SuspiciousFcSnModal,
  type FcSnCandidate,
} from './SuspiciousFcSnModal';
import styles from './ImportFlightLogModal.module.css';

type ImportStep = 'upload' | 'preview' | 'select' | 'success';

interface ImportFlightLogModalProps {
  onClose: () => void;
}

function formatDuration(seconds: number): string {
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}m ${String(s).padStart(2, '0')}s`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${String(m).padStart(2, '0')}m`;
}

interface MatchInfo {
  /** Show this row at all? Hidden when the log carries no SN for this slot. */
  visible: boolean;
  /** Green ✓ when the backend matched a row (exact or prefix-tolerance);
   *  amber ⚠ when no match was found and auto-register will fire. */
  ok: boolean;
  text: string;
}

function describeMatch(
  match: EquipmentMatch | null,
  logSerial: string | null | undefined,
  preferName: string | null | undefined,
  label: string,
): MatchInfo {
  if (!logSerial) {
    return { visible: false, ok: false, text: '' };
  }
  if (match) {
    const idLabel = match.equipment_id ?? match.serial_number;
    if (match.prefix_matched) {
      return {
        visible: true,
        ok: true,
        text: `Matched via prefix tolerance: ${idLabel} (log SN '${logSerial}' was truncated; canonical SN: ${match.serial_number})`,
      };
    }
    return { visible: true, ok: true, text: `Matched: ${idLabel}` };
  }
  const display = preferName ? `${preferName} (SN: ${logSerial})` : logSerial;
  return {
    visible: true,
    ok: false,
    text: `${label} not registered: ${display} — will be auto-registered on import`,
  };
}

/** Mirror of `services::sn_matching::EquipmentMatch` in the backend. */
interface EquipmentMatch {
  id: number;
  /** e.g. "UAV-020" — null on rows that pre-date the equipment-ID system. */
  equipment_id: string | null;
  /** Canonical SN as stored in the DB; differs from the log SN by 1–2
   *  trailing chars when `prefix_matched` is true. */
  serial_number: string;
  /** True when the lookup hit via prefix-tolerance fallback (log SN was
   *  truncated, typically by the dji-log-parser crate). */
  prefix_matched: boolean;
}

interface PreviewPayload {
  aircraft_name?: string;
  /** Canonical aircraft_models.name resolved by the backend from the
   *  log's drone_type_code. Distinct from `aircraft_name`, which is the
   *  DJI app's operator-set device label (often a nickname). Null when
   *  the drone-type variant has no unambiguous catalog mapping. */
  aircraft_model?: string;
  aircraft_serial?: string;
  battery_serial?: string;
  duration_seconds?: number;
  max_altitude_m?: number;
  max_distance_m?: number;
  total_distance_m?: number;
  trajectory?: Array<{ lat: number; lon: number; alt: number; t_sec: number }>;
  photo_count?: number;
  video_seconds?: number;
  app_source?: string;
  app_version?: string;
  rc_serial?: string;
  home_lat?: number;
  home_lon?: number;
  /** Backend-computed: existing aircraft row that the log SN resolves to
   *  (exact or prefix-tolerance match). Null when the SN is new to the
   *  org and will be auto-registered on commit. */
  aircraft_match?: EquipmentMatch | null;
  battery_match?: EquipmentMatch | null;
  controller_match?: EquipmentMatch | null;
}

interface UploadResponse {
  preview: PreviewPayload;
}

/**
 * One arm→disarm flight segment from an ArduPilot `.tlog`. The backend
 * flattens the FlightPreview, so all PreviewPayload fields appear at the
 * segment's top level alongside these ArduPilot-specific extras.
 */
interface ArduPilotSegment extends PreviewPayload {
  /** Backend heuristic: pre-check this row when true (real flight vs bench). */
  looks_like_flight: boolean;
  armed_seconds: number;
  /** A live flight with this guid/source_filename already exists → disabled. */
  already_imported: boolean;
  flight_guid?: string;
  flight_start?: string;
  flight_end?: string;
  satellite_count_min?: number;
  min_battery_pct?: number;
  source_filename?: string;
}

/** Response of POST /api/v1/flights/import-ardupilot. */
interface ArduPilotUploadResponse {
  filename: string;
  /** Shared airframe resolved from the board UID, or null if unrecognised. */
  aircraft_match: EquipmentMatch | null;
  segments: ArduPilotSegment[];
}

/**
 * Drag-drop import flow for flight logs (DJI Fly .txt, DJI Fly / AirData /
 * Litchi CSV). Uploads, shows a preview with map + EASA debrief form, then
 * commits. Handles auto-registration of unrecognised aircraft and batteries.
 */
export function ImportFlightLogModal({ onClose }: ImportFlightLogModalProps) {
  const queryClient = useQueryClient();
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Keeps the uploaded File alive across the parse→commit→hand-off flow so it
  // can be attached to an incident report after the ArduPilot commit.
  const uploadedFileRef = useRef<File | null>(null);
  const { data: aircraft } = useQuery({
    queryKey: ['aircraft', true],
    queryFn: () => fetchAircraft(true),
  });
  const { data: pilots } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });

  const [step, setStep] = useState<ImportStep>('upload');
  const [isUploading, setIsUploading] = useState(false);
  const [isCommitting, setIsCommitting] = useState(false);
  const [error, setError] = useState('');
  const [previewData, setPreviewData] = useState<UploadResponse | null>(null);

  // ArduPilot .tlog multi-flight review state. `ardupilot` holds the parsed
  // segments; `selectedSegments` are indices into it queued for commit;
  // `focusedSegment` is the segment whose track the map shows; `commitSummary`
  // distinguishes the multi-flight success panel from the DJI single-flight one.
  const [ardupilot, setArdupilot] = useState<ArduPilotUploadResponse | null>(null);
  // Which ArduPilot log format was uploaded. Drives the commit `log_source`:
  // `.bin` is authoritative (precedence 20, wins over a matching `.tlog`);
  // `.tlog` stays the telemetry tier (precedence 10).
  const [arduLogSource, setArduLogSource] = useState<
    'ardupilot_tlog' | 'ardupilot_bin'
  >('ardupilot_tlog');
  const [selectedSegments, setSelectedSegments] = useState<Set<number>>(new Set());
  const [focusedSegment, setFocusedSegment] = useState<number>(0);
  const [commitSummary, setCommitSummary] = useState<{
    imported: number;
    skipped: number;
    failed: number;
  } | null>(null);

  // "Report an incident for this flight" hand-off (ArduPilot select step only).
  // The checkbox is only meaningful when exactly one segment is selected.
  const [reportIncident, setReportIncident] = useState(false);
  // Set after a successful single-segment commit when reportIncident is on;
  // presence opens the incident modal instead of the plain success panel.
  const [incidentHandoff, setIncidentHandoff] = useState<{
    flightId: number;
    aircraftId?: number;
    occurredAt?: string;
  } | null>(null);

  // EASA debrief fields
  const [aircraftId, setAircraftId] = useState('');
  const [batteryId, setBatteryId] = useState('');
  const [pilotId, setPilotId] = useState('');
  const [easaCategory, setEasaCategory] = useState('');
  const [missionType, setMissionType] = useState('');
  const [remarks, setRemarks] = useState('');
  const [preflightDone, setPreflightDone] = useState(false);

  // Auto-register flags — pre-checked only when the backend reported NO
  // match for the log's serial (i.e. preview returned `*_match: null`).
  // Match notes are derived from `previewData.preview.*_match` rather
  // than computed client-side, so the modal cannot disagree with what
  // commit will actually do.
  const [autoRegisterAircraft, setAutoRegisterAircraft] = useState(false);
  const [autoRegisterBattery, setAutoRegisterBattery] = useState(false);

  // Populated when the commit endpoint returns 409 / "suspicious_fc_sn".
  // Opens SuspiciousFcSnModal as an overlay over the preview stage; the
  // operator either links the unmatched FC-SN to an existing aircraft or
  // force-registers a new one after typing a reason. Cancel returns here
  // with all debrief inputs intact — no backend cleanup needed because
  // the preflight check fires before the flight INSERT.
  const [suspiciousData, setSuspiciousData] = useState<null | {
    sn: string;
    modelName: string;
    candidates: FcSnCandidate[];
  }>(null);

  // Sorted aircraft list for the dropdown — alphanumeric on equipment_id,
  // with rows that have no equipment_id sinking to the bottom.
  const sortedAircraft = useMemo<Aircraft[]>(() => {
    if (!aircraft) return [];
    return [...aircraft].sort((a, b) => {
      if (!a.equipment_id && !b.equipment_id) return 0;
      if (!a.equipment_id) return 1;
      if (!b.equipment_id) return -1;
      return a.equipment_id.localeCompare(b.equipment_id, undefined, {
        numeric: true,
      });
    });
  }, [aircraft]);

  const selectedAircraft = useMemo<Aircraft | undefined>(() => {
    if (!aircraftId || !aircraft) return undefined;
    return aircraft.find((a: Aircraft) => String(a.id) === aircraftId);
  }, [aircraftId, aircraft]);

  // An incident can only be attached to a single flight, so drop the flag
  // whenever the selection is no longer exactly one segment.
  useEffect(() => {
    if (reportIncident && selectedSegments.size !== 1) {
      setReportIncident(false);
    }
  }, [reportIncident, selectedSegments]);

  const handleUpload = async (file: File) => {
    uploadedFileRef.current = file;
    setIsUploading(true);
    setError('');
    const formData = new FormData();
    formData.append('file', file);
    try {
      const token = localStorage.getItem('access_token');

      // ArduPilot .tlog / .bin: one file → many arm/disarm segments. Route to
      // the dedicated endpoint and open the segment-review step. `.bin`
      // (DataFlash) is authoritative; `.tlog` (Mission Planner telemetry) is
      // the exception. (The DJI .txt/.csv path below is untouched.)
      const lowerName = file.name.toLowerCase();
      if (lowerName.endsWith('.tlog') || lowerName.endsWith('.bin')) {
        setArduLogSource(
          lowerName.endsWith('.bin') ? 'ardupilot_bin' : 'ardupilot_tlog',
        );
        const arduUrl = import.meta.env.DEV
          ? `${window.location.protocol}//${window.location.hostname}:3000/api/v1/flights/import-ardupilot`
          : '/api/v1/flights/import-ardupilot';
        const arduRes = await fetch(arduUrl, {
          method: 'POST',
          headers: token ? { Authorization: `Bearer ${token}` } : {},
          body: formData,
        });
        const arduData = await arduRes.json();
        if (!arduRes.ok) throw new Error(arduData.error ?? 'Upload failed');
        const resp = arduData as ArduPilotUploadResponse;
        setArdupilot(resp);
        // Pre-select real flights (looks_like_flight) that aren't duplicates;
        // focus the map on the first real flight (or the first segment).
        const preselect = new Set<number>();
        resp.segments.forEach((seg, i) => {
          if (seg.looks_like_flight && !seg.already_imported) preselect.add(i);
        });
        setSelectedSegments(preselect);
        const firstFlight = resp.segments.findIndex((s) => s.looks_like_flight);
        setFocusedSegment(firstFlight >= 0 ? firstFlight : 0);
        setStep('select');
        return;
      }

      const uploadUrl = import.meta.env.DEV
        ? `${window.location.protocol}//${window.location.hostname}:3000/api/v1/flights/import`
        : '/api/v1/flights/import';
      const res = await fetch(uploadUrl, {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: formData,
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? 'Upload failed');
      setPreviewData(data);

      // The backend already ran sn_matching::find_match against the org's
      // existing rows (exact-then-prefix-tolerance). Use those results
      // directly: select the matched aircraft in the dropdown when one
      // exists, and pre-check auto-register only when no match was found.
      const prev = data.preview as PreviewPayload;

      if (prev.aircraft_match) {
        setAircraftId(String(prev.aircraft_match.id));
        setAutoRegisterAircraft(false);
      } else if (prev.aircraft_serial) {
        setAutoRegisterAircraft(true);
      }

      if (prev.battery_match) {
        setBatteryId(String(prev.battery_match.id));
        setAutoRegisterBattery(false);
      } else if (prev.battery_serial) {
        setAutoRegisterBattery(true);
      }

      setStep('preview');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Upload failed');
    } finally {
      setIsUploading(false);
    }
  };

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) handleUpload(file);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    const file = e.dataTransfer.files?.[0];
    if (file) handleUpload(file);
  };

  // Assembles the JSON body sent to /api/v1/flights/commit. Extras are
  // spread last so retry paths (link_existing_aircraft_id /
  // force_register_new + force_register_reason) can override / extend
  // the base shape without duplicating the field list.
  const buildCommitBody = (extras: Record<string, unknown> = {}) => ({
    preview: previewData?.preview,
    aircraft_id: aircraftId ? Number(aircraftId) : null,
    battery_id: batteryId ? Number(batteryId) : null,
    pilot_id: pilotId ? Number(pilotId) : null,
    easa_category: easaCategory,
    mission_type: missionType || null,
    remarks: remarks || null,
    preflight_done: preflightDone,
    auto_register_aircraft: autoRegisterAircraft,
    // new_aircraft_name = DJI app's operator-set label (used only as
    // nickname). new_aircraft_model = canonical catalog name resolved
    // from the log's drone_type_code (used as catalog lookup key —
    // backend rejects unknown names rather than creating junk rows).
    new_aircraft_name: previewData?.preview.aircraft_name ?? null,
    new_aircraft_model: previewData?.preview.aircraft_model ?? null,
    new_aircraft_serial: previewData?.preview.aircraft_serial ?? null,
    auto_register_battery: autoRegisterBattery,
    new_battery_serial: previewData?.preview.battery_serial ?? null,
    // batteries.compatible_model carries the canonical aircraft model
    // (the drone the pack is for), not the device's nickname. Was
    // previously bound to aircraft_name — same bug, same line.
    new_battery_model: previewData?.preview.aircraft_model ?? null,
    ...extras,
  });

  const authHeaders = (): HeadersInit => {
    const token = localStorage.getItem('access_token');
    return {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    };
  };

  const handleCommit = async () => {
    if (!easaCategory) {
      setError('EASA category is required');
      return;
    }
    setIsCommitting(true);
    setError('');
    try {
      const res = await fetch('/api/v1/flights/commit', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify(buildCommitBody()),
      });
      const data = await res.json();
      // FC-SN-prone refusal — branch BEFORE the generic !res.ok throw so
      // the operator gets the structured candidate-list UI rather than a
      // flat error message. Stay on the preview stage; the modal opens on
      // top via the suspiciousData state.
      if (res.status === 409 && data?.kind === 'suspicious_fc_sn') {
        setSuspiciousData({
          sn: data.sn,
          modelName: data.model_name,
          candidates: data.candidates ?? [],
        });
        return;
      }
      if (!res.ok) throw new Error(data.error ?? 'Commit failed');
      setStep('success');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Commit failed');
    } finally {
      setIsCommitting(false);
    }
  };

  // Retry handlers passed to SuspiciousFcSnModal. Both throw on backend
  // error so the modal's own try/catch can render the message inline. On
  // success they clear suspiciousData (unmounts the modal) and transition
  // this modal to 'success', mirroring the happy-path commit flow.
  const handleSuspiciousLink = async (aircraftIdToLink: number) => {
    const res = await fetch('/api/v1/flights/commit', {
      method: 'POST',
      headers: authHeaders(),
      body: JSON.stringify(
        buildCommitBody({ link_existing_aircraft_id: aircraftIdToLink }),
      ),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error ?? 'Link failed');
    setSuspiciousData(null);
    setStep('success');
    showToast('Flight linked to existing aircraft', 'success');
  };

  const handleSuspiciousForceRegister = async (reason: string) => {
    const res = await fetch('/api/v1/flights/commit', {
      method: 'POST',
      headers: authHeaders(),
      body: JSON.stringify(
        buildCommitBody({
          force_register_new: true,
          force_register_reason: reason,
        }),
      ),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error ?? 'Force-register failed');
    setSuspiciousData(null);
    setStep('success');
    showToast('New aircraft registered', 'success');
  };

  // --- ArduPilot segment selection + multi-commit ---

  const toggleSegment = (i: number, checked: boolean) => {
    setSelectedSegments((prev) => {
      const next = new Set(prev);
      if (checked) next.add(i);
      else next.delete(i);
      return next;
    });
  };

  const selectAllSegments = () => {
    if (!ardupilot) return;
    const next = new Set<number>();
    ardupilot.segments.forEach((s, i) => {
      if (!s.already_imported) next.add(i);
    });
    setSelectedSegments(next);
  };

  const selectNoneSegments = () => setSelectedSegments(new Set());

  // Commit body for ONE ArduPilot segment. The board UID drives the airframe:
  // matched → link by id; unmatched → auto-register (once, on the first commit).
  const buildArduCommitBody = (segment: ArduPilotSegment) => ({
    preview: segment,
    aircraft_id: ardupilot?.aircraft_match ? ardupilot.aircraft_match.id : null,
    battery_id: null,
    pilot_id: pilotId ? Number(pilotId) : null,
    easa_category: easaCategory,
    mission_type: missionType || null,
    remarks: remarks || null,
    preflight_done: preflightDone,
    // `.bin` → precedence 20 (bin-wins merge); `.tlog` → 10.
    log_source: arduLogSource,
    auto_register_aircraft: !ardupilot?.aircraft_match,
    new_aircraft_serial: segment.aircraft_serial ?? null,
    new_aircraft_name: segment.aircraft_name ?? null,
    new_aircraft_model: null,
    auto_register_battery: false,
  });

  const handleArduCommit = async () => {
    if (!easaCategory) {
      setError('EASA category is required');
      return;
    }
    if (!ardupilot) return;
    setIsCommitting(true);
    setError('');
    let imported = 0;
    let skipped = 0;
    let failed = 0;
    let firstFailure = '';
    // Incident hand-off is only offered for a single-segment selection; capture
    // the committed flight id from that segment's successful commit response.
    const handoffEnabled = reportIncident && selectedSegments.size === 1;
    let committedFlightId: number | null = null;
    try {
      // SEQUENTIAL on purpose: the shared autopilot UID must auto-register
      // exactly once on the first commit; the rest then match it via
      // find_match. Parallel requests would race and create duplicate aircraft.
      const indices = Array.from(selectedSegments).sort((a, b) => a - b);
      for (const idx of indices) {
        const segment = ardupilot.segments[idx];
        if (!segment) continue;
        try {
          const res = await fetch('/api/v1/flights/commit', {
            method: 'POST',
            headers: authHeaders(),
            body: JSON.stringify(buildArduCommitBody(segment)),
          });
          const data = await res.json().catch(() => ({}));
          if (res.ok) {
            imported++;
            if (handoffEnabled && typeof data?.id === 'number') {
              committedFlightId = data.id;
            }
          } else if (
            res.status === 400 &&
            typeof data?.error === 'string' &&
            data.error.toLowerCase().includes('already been imported')
          ) {
            skipped++;
          } else {
            failed++;
            if (!firstFailure) {
              firstFailure = data?.error ?? `Commit failed (HTTP ${res.status})`;
            }
          }
        } catch (err) {
          failed++;
          if (!firstFailure) {
            firstFailure = err instanceof Error ? err.message : 'Commit failed';
          }
        }
      }
      if (firstFailure) setError(firstFailure);
      setCommitSummary({ imported, skipped, failed });
      setStep('success');
      // Hand off to the incident report instead of the plain success panel
      // when the single selected flight committed cleanly.
      if (handoffEnabled && committedFlightId != null) {
        const idx = Array.from(selectedSegments)[0];
        const segment = ardupilot.segments[idx];
        setIncidentHandoff({
          flightId: committedFlightId,
          aircraftId: ardupilot.aircraft_match?.id,
          occurredAt: segment?.flight_start,
        });
      }
    } finally {
      setIsCommitting(false);
    }
  };

  const handleClose = () => {
    if (step === 'success') {
      // Invalidate caches affected by a successful DJI import:
      // - new flight row + denorm counter bump on the matched aircraft
      // - potential auto-registration of aircraft / battery / controller
      queryClient.invalidateQueries({ queryKey: ['flights'] });
      queryClient.invalidateQueries({ queryKey: ['flight-stats'] });
      queryClient.invalidateQueries({ queryKey: ['flight-filter-options'] });
      queryClient.invalidateQueries({ queryKey: ['aircraft'] });
      queryClient.invalidateQueries({ queryKey: ['aircraft-detail'] });
      queryClient.invalidateQueries({ queryKey: ['aircraft-flights'] });
      queryClient.invalidateQueries({ queryKey: ['batteries'] });
      queryClient.invalidateQueries({ queryKey: ['controllers'] });
      queryClient.invalidateQueries({ queryKey: ['fleet-stats'] });
      queryClient.invalidateQueries({ queryKey: ['dashboard'] });
      // The incident hand-off can create a new incident on the way out.
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      // FC-SN link / force-register paths write to audit_log; refresh the
      // viewer so LINK_FC_SN_IDENTIFIER and FORCE_REGISTER_FC_SN_PRONE
      // rows appear immediately for admins reviewing import history.
      queryClient.invalidateQueries({ queryKey: ['audit-log'] });
    }
    onClose();
  };

  const preview = previewData?.preview;
  const trajectory = preview?.trajectory ?? [];

  // Derive match notes purely from the backend's match results so the
  // modal always reflects what commit will actually do (preview cannot
  // disagree with commit because both call the same `find_match`).
  const aircraftMatchInfo = describeMatch(
    preview?.aircraft_match ?? null,
    preview?.aircraft_serial,
    preview?.aircraft_name,
    'Aircraft',
  );
  const batteryMatchInfo = describeMatch(
    preview?.battery_match ?? null,
    preview?.battery_serial,
    null,
    'Battery',
  );
  const controllerMatchInfo = describeMatch(
    preview?.controller_match ?? null,
    preview?.rc_serial,
    null,
    'Controller',
  );

  const totalDistance = Number(preview?.total_distance_m ?? 0);
  const distanceLabel = totalDistance < 1000
    ? `${totalDistance.toFixed(0)} m`
    : `${(totalDistance / 1000).toFixed(2)} km`;

  return (
    <>
    <div className={styles.overlay} onClick={handleClose}>
      <div
        className={`${styles.modal} ${
          step === 'preview' || step === 'select' ? styles.modalWide : ''
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={styles.title}>Import flight log</h2>

        {step === 'upload' && (
          <div>
            <div
              onDragOver={(e) => e.preventDefault()}
              onDrop={handleDrop}
              onClick={() => fileInputRef.current?.click()}
              className={styles.dropzone}
            >
              <p className={styles.dropzonePrimary}>
                {isUploading ? 'Parsing file…' : 'Drop log file here or click to browse'}
              </p>
              <p className={styles.dropzoneHint}>
                Accepts .txt (DJI Fly encrypted), .csv (DJI Fly / AirData / Litchi),
                and ArduPilot .tlog/.bin (Mission Planner telemetry / DataFlash)
              </p>
              <p className={styles.dropzoneNote}>
                Note: .txt files require a DJI Developer API key configured in Settings.
                .tlog/.bin files contain one or more flights (each arm→disarm); .bin is
                authoritative and upgrades a matching .tlog on import.
              </p>
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept=".txt,.csv,.tlog,.bin"
              onChange={handleFileSelect}
              className={styles.fileInput}
            />
            {error && <p className={styles.error}>{error}</p>}
            <div className={styles.actionsRight}>
              <button type="button" onClick={handleClose} className={styles.btnSecondary}>
                Cancel
              </button>
            </div>
          </div>
        )}

        {step === 'preview' && preview && (
          <div className={styles.previewLayout}>
            <div className={styles.mapColumn}>
              <FlightMap
                trajectory={trajectory}
                homeLat={preview.home_lat}
                homeLon={preview.home_lon}
                height="520px"
              />
            </div>

            <div className={styles.formColumn}>
              <div className={styles.previewGrid}>
                <div>
                  <span className={styles.previewLabel}>Aircraft</span>
                  <br />
                  <span className={styles.previewValue}>
                    {preview.aircraft_name ?? preview.aircraft_serial ?? 'Unknown'}
                  </span>
                </div>
                <div>
                  <span className={styles.previewLabel}>Battery</span>
                  <br />
                  <span className={styles.previewValue}>
                    {preview.battery_serial ?? '—'}
                  </span>
                </div>
                <div>
                  <span className={styles.previewLabel}>Duration</span>
                  <br />
                  <span className={styles.previewValue}>
                    {formatDuration(Number(preview.duration_seconds ?? 0))}
                  </span>
                </div>
                <div>
                  <span className={styles.previewLabel}>Max Altitude</span>
                  <br />
                  <span className={styles.previewValue}>
                    {Number(preview.max_altitude_m ?? 0).toFixed(0)} m
                  </span>
                </div>
                <div>
                  <span className={styles.previewLabel}>Max Distance</span>
                  <br />
                  <span className={styles.previewValue}>
                    {Number(preview.max_distance_m ?? 0).toFixed(0)} m
                  </span>
                </div>
                <div>
                  <span className={styles.previewLabel}>Total Distance</span>
                  <br />
                  <span className={styles.previewValue}>{distanceLabel}</span>
                </div>
                <div>
                  <span className={styles.previewLabel}>GPS Points</span>
                  <br />
                  <span className={styles.previewValue}>{trajectory.length}</span>
                </div>
              </div>

              <p className={styles.metaLine}>
                {Number(preview.photo_count ?? 0)} photos | {Number(preview.video_seconds ?? 0)}s video
                {preview.app_source
                  ? ` | Source: ${preview.app_source}`
                  : preview.app_version
                  ? ` | ${preview.app_version}`
                  : ''}
              </p>
              <p className={styles.metaLine}>
                {controllerMatchInfo.visible ? (
                  <>
                    Controller:{' '}
                    <span className={styles.metaLineValue}>{preview.rc_serial}</span>
                    {' — '}
                    <span
                      className={
                        controllerMatchInfo.ok ? styles.matchOk : styles.matchWarn
                      }
                    >
                      {controllerMatchInfo.ok ? '✓ ' : '⚠ '}
                      {controllerMatchInfo.text}
                    </span>
                  </>
                ) : (
                  'Controller: not detected'
                )}
              </p>

              <p className={styles.sectionHeading}>EASA Debrief</p>

              <div className={styles.field}>
                <label className={styles.label}>Aircraft</label>
                <select
                  className={styles.input}
                  value={aircraftId}
                  onChange={(e) => setAircraftId(e.target.value)}
                >
                  <option value="">— Select —</option>
                  {sortedAircraft.map((a: Aircraft) => {
                    const warn = a.needs_manual_sn ? ' ⚠' : '';
                    return (
                      <option key={a.id} value={a.id}>
                        {formatAircraftLabel(a)}
                        {warn}
                      </option>
                    );
                  })}
                </select>
                {aircraftMatchInfo.visible && (
                  <span
                    className={`${styles.matchNote} ${
                      aircraftMatchInfo.ok ? styles.matchOk : styles.matchWarn
                    }`}
                  >
                    {aircraftMatchInfo.ok ? '✓ ' : '⚠ '}
                    {aircraftMatchInfo.text}
                  </span>
                )}
                {selectedAircraft?.needs_manual_sn && (
                  <span className={`${styles.matchNote} ${styles.matchWarn}`}>
                    ⚠ This aircraft has no verified serial number. Confirm
                    you've selected the correct individual before importing.
                  </span>
                )}
              </div>
              {batteryMatchInfo.visible && (
                <p
                  className={`${styles.batMatch} ${
                    batteryMatchInfo.ok ? styles.matchOk : styles.matchWarn
                  }`}
                >
                  {batteryMatchInfo.ok ? '✓ ' : '⚠ '}
                  {batteryMatchInfo.text}
                </p>
              )}
              <div className={styles.field}>
                <label className={styles.label}>Pilot (PIC)</label>
                <select
                  className={styles.input}
                  value={pilotId}
                  onChange={(e) => setPilotId(e.target.value)}
                >
                  <option value="">— Select —</option>
                  {(pilots ?? [])
                    .filter((p: Pilot) => p.is_active)
                    .map((p: Pilot) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                </select>
              </div>
              <div className={styles.row2}>
                <div className={styles.field}>
                  <label className={styles.label}>EASA Category *</label>
                  <select
                    className={styles.input}
                    value={easaCategory}
                    onChange={(e) => setEasaCategory(e.target.value)}
                    required
                  >
                    <option value="">— Select —</option>
                    <option value="Open-A1">Open-A1</option>
                    <option value="Open-A2">Open-A2</option>
                    <option value="Open-A3">Open-A3</option>
                    <option value="Specific-SAIL-I">Specific-SAIL-I</option>
                    <option value="Specific-SAIL-II">Specific-SAIL-II</option>
                  </select>
                </div>
                <div className={styles.field}>
                  <label className={styles.label}>Mission Type</label>
                  <select
                    className={styles.input}
                    value={missionType}
                    onChange={(e) => setMissionType(e.target.value)}
                  >
                    <option value="">— Select —</option>
                    <option value="Photo/Film">Photo/Film</option>
                    <option value="Survey">Survey</option>
                    <option value="Inspection">Inspection</option>
                    <option value="Training">Training</option>
                    <option value="Free Flight">Free Flight</option>
                    <option value="Test Flight">Test Flight</option>
                  </select>
                </div>
              </div>
              <div className={styles.field}>
                <label className={styles.label}>Remarks</label>
                <textarea
                  className={styles.textarea}
                  value={remarks}
                  onChange={(e) => setRemarks(e.target.value)}
                  placeholder="Optional notes…"
                />
              </div>
              <div className={styles.checkboxRow}>
                <input
                  type="checkbox"
                  id="importPreflight"
                  checked={preflightDone}
                  onChange={(e) => setPreflightDone(e.target.checked)}
                  className={styles.checkbox}
                />
                <label htmlFor="importPreflight" className={styles.checkboxLabel}>
                  Preflight checklist completed
                </label>
              </div>
              {error && <p className={styles.error}>{error}</p>}
              <div className={styles.actionsRight}>
                <button
                  type="button"
                  onClick={handleClose}
                  className={styles.btnSecondary}
                >
                  Cancel
                </button>
                <button
                  onClick={handleCommit}
                  disabled={isCommitting}
                  className={styles.btnPrimary}
                >
                  {isCommitting ? 'Importing…' : 'Confirm Import'}
                </button>
              </div>
            </div>
          </div>
        )}

        {step === 'select' && ardupilot && (
          <div className={styles.previewLayout}>
            <div className={styles.mapColumn}>
              <FlightMap
                trajectory={ardupilot.segments[focusedSegment]?.trajectory ?? []}
                homeLat={ardupilot.segments[focusedSegment]?.home_lat}
                homeLon={ardupilot.segments[focusedSegment]?.home_lon}
                height="520px"
              />
            </div>

            <div className={styles.formColumn}>
              {ardupilot.aircraft_match ? (
                <p className={`${styles.matchNote} ${styles.matchOk}`}>
                  ✓ Autopilot recognised:{' '}
                  {ardupilot.aircraft_match.equipment_id ??
                    ardupilot.aircraft_match.serial_number}
                </p>
              ) : (
                <p className={`${styles.matchNote} ${styles.matchWarn}`}>
                  ⚠ Unrecognised autopilot (UID{' '}
                  {ardupilot.segments[0]?.aircraft_serial ?? '—'}) — will be
                  registered as a new aircraft you can merge later
                </p>
              )}

              <div className={styles.segmentListHeader}>
                <span className={styles.sectionHeadingInline}>
                  Flights in this log ({ardupilot.segments.length})
                </span>
                <span className={styles.selectAllActions}>
                  <button
                    type="button"
                    className={styles.linkButton}
                    onClick={selectAllSegments}
                  >
                    Select all
                  </button>
                  <span className={styles.selectAllSep}> / </span>
                  <button
                    type="button"
                    className={styles.linkButton}
                    onClick={selectNoneSegments}
                  >
                    none
                  </button>
                </span>
              </div>

              <div className={styles.segmentList}>
                {ardupilot.segments.map((seg, i) => {
                  const isFocused = i === focusedSegment;
                  const checked = selectedSegments.has(i);
                  const dup = seg.already_imported;
                  return (
                    <div
                      key={seg.flight_guid ?? seg.source_filename ?? i}
                      className={`${styles.segmentRow} ${
                        isFocused ? styles.segmentRowFocused : ''
                      } ${dup ? styles.segmentRowDisabled : ''}`}
                      onClick={() => setFocusedSegment(i)}
                    >
                      <div className={styles.segmentCheck}>
                        <input
                          type="checkbox"
                          className={styles.checkbox}
                          checked={checked}
                          disabled={dup}
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => toggleSegment(i, e.target.checked)}
                        />
                      </div>
                      <div className={styles.segmentBody}>
                        <div className={styles.segmentTopline}>
                          <span className={styles.segmentTime}>
                            {seg.flight_start
                              ? formatDateTime(seg.flight_start)
                              : `Segment ${i + 1}`}
                          </span>
                          {dup && (
                            <span className={styles.badgeDuplicate}>
                              Already imported
                            </span>
                          )}
                        </div>
                        <div className={styles.segmentStats}>
                          {formatDuration(
                            Number(seg.duration_seconds ?? seg.armed_seconds ?? 0),
                          )}
                          {' · '}
                          {Number(seg.max_altitude_m ?? 0).toFixed(0)} m alt
                          {' · '}
                          {Number(seg.max_distance_m ?? 0).toFixed(0)} m dist
                          {' · '}
                          {seg.trajectory?.length ?? 0} GPS pts
                        </div>
                        {!seg.looks_like_flight && !dup && (
                          <div className={styles.segmentHint}>
                            Looks like a ground/bench test
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>

              <p className={styles.sectionHeading}>EASA Debrief</p>

              <div className={styles.field}>
                <label className={styles.label}>Pilot (PIC)</label>
                <select
                  className={styles.input}
                  value={pilotId}
                  onChange={(e) => setPilotId(e.target.value)}
                >
                  <option value="">— Select —</option>
                  {(pilots ?? [])
                    .filter((p: Pilot) => p.is_active)
                    .map((p: Pilot) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                </select>
              </div>
              <div className={styles.row2}>
                <div className={styles.field}>
                  <label className={styles.label}>EASA Category *</label>
                  <select
                    className={styles.input}
                    value={easaCategory}
                    onChange={(e) => setEasaCategory(e.target.value)}
                    required
                  >
                    <option value="">— Select —</option>
                    <option value="Open-A1">Open-A1</option>
                    <option value="Open-A2">Open-A2</option>
                    <option value="Open-A3">Open-A3</option>
                    <option value="Specific-SAIL-I">Specific-SAIL-I</option>
                    <option value="Specific-SAIL-II">Specific-SAIL-II</option>
                  </select>
                </div>
                <div className={styles.field}>
                  <label className={styles.label}>Mission Type</label>
                  <select
                    className={styles.input}
                    value={missionType}
                    onChange={(e) => setMissionType(e.target.value)}
                  >
                    <option value="">— Select —</option>
                    <option value="Photo/Film">Photo/Film</option>
                    <option value="Survey">Survey</option>
                    <option value="Inspection">Inspection</option>
                    <option value="Training">Training</option>
                    <option value="Free Flight">Free Flight</option>
                    <option value="Test Flight">Test Flight</option>
                  </select>
                </div>
              </div>
              <div className={styles.field}>
                <label className={styles.label}>Remarks</label>
                <textarea
                  className={styles.textarea}
                  value={remarks}
                  onChange={(e) => setRemarks(e.target.value)}
                  placeholder="Optional notes (applied to all selected flights)…"
                />
              </div>
              <div className={styles.checkboxRow}>
                <input
                  type="checkbox"
                  id="arduPreflight"
                  checked={preflightDone}
                  onChange={(e) => setPreflightDone(e.target.checked)}
                  className={styles.checkbox}
                />
                <label htmlFor="arduPreflight" className={styles.checkboxLabel}>
                  Preflight checklist completed
                </label>
              </div>
              <div className={styles.checkboxRow}>
                <input
                  type="checkbox"
                  id="arduReportIncident"
                  checked={reportIncident}
                  disabled={selectedSegments.size !== 1}
                  onChange={(e) => setReportIncident(e.target.checked)}
                  className={styles.checkbox}
                />
                <label
                  htmlFor="arduReportIncident"
                  className={styles.checkboxLabel}
                >
                  Report an incident for this flight
                </label>
              </div>
              {selectedSegments.size !== 1 && (
                <p className={styles.checkboxHint}>
                  Select a single flight to attach an incident
                </p>
              )}
              {error && <p className={styles.error}>{error}</p>}
              <div className={styles.actionsRight}>
                <button
                  type="button"
                  onClick={handleClose}
                  className={styles.btnSecondary}
                >
                  Cancel
                </button>
                <button
                  onClick={handleArduCommit}
                  disabled={selectedSegments.size === 0 || isCommitting}
                  className={styles.btnPrimary}
                >
                  {isCommitting
                    ? 'Importing…'
                    : `Import ${selectedSegments.size} selected flight${
                        selectedSegments.size === 1 ? '' : 's'
                      }`}
                </button>
              </div>
            </div>
          </div>
        )}

        {step === 'success' && !incidentHandoff && (
          <div className={styles.successPanel}>
            {commitSummary ? (
              <p className={styles.successText}>
                Imported {commitSummary.imported} flight
                {commitSummary.imported === 1 ? '' : 's'}.
                {commitSummary.skipped > 0 &&
                  `, ${commitSummary.skipped} skipped (already imported)`}
                {commitSummary.failed > 0 && (
                  <span className={styles.matchWarn}>
                    , {commitSummary.failed} failed
                  </span>
                )}
              </p>
            ) : (
              <p className={styles.successText}>Flight imported successfully!</p>
            )}
            <button onClick={handleClose} className={styles.btnPrimary}>
              Close
            </button>
          </div>
        )}
      </div>
    </div>
    {suspiciousData && (
      <SuspiciousFcSnModal
        sn={suspiciousData.sn}
        modelName={suspiciousData.modelName}
        candidates={suspiciousData.candidates}
        onLink={handleSuspiciousLink}
        onForceRegister={handleSuspiciousForceRegister}
        onCancel={() => setSuspiciousData(null)}
      />
    )}
    {incidentHandoff && (
      <ReportIncidentModal
        aircraft={aircraft ?? []}
        pilots={pilots ?? []}
        initialFlightId={incidentHandoff.flightId}
        initialAircraftId={incidentHandoff.aircraftId}
        initialOccurredAt={incidentHandoff.occurredAt}
        attachFile={uploadedFileRef.current ?? undefined}
        // Cancel or success both finalise the (already-committed) import so
        // caches invalidate and the imported flights appear.
        onClose={handleClose}
        onCreated={() => showToast('Incident reported', 'success')}
      />
    )}
    </>
  );
}
