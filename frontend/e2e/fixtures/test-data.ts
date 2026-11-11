// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { execFileSync } from 'node:child_process';
import type { APIRequestContext } from '@playwright/test';
import { API_BASE } from './auth';

/**
 * Clear the per-username login rate-limit window for an email. The backend
 * enforces a hardcoded 10-attempt / 15-minute rolling window per identifier
 * (see backend/src/services/rate_limit.rs), which E2E runs hit quickly when
 * iterating on tests. Cleared via direct psql for the same reason as
 * flagAircraftNeedsManualSn: no API surface, dev-only utility.
 */
export function clearLoginRateLimit(email: string): void {
  // Match the backend's identifier hygiene: lowercase + trim.
  const id = email.trim().toLowerCase().replace(/'/g, "''");
  runPsql(`DELETE FROM auth_attempts WHERE scope = 'login' AND identifier = '${id}';`);
}

interface AircraftModel {
  id: number;
  name: string;
  is_active: boolean;
}

interface Aircraft {
  id: number;
  serial_number: string;
  equipment_id: string | null;
  model_id: number | null;
  status: string;
  needs_manual_sn: boolean;
}

interface CreateOpts {
  equipment_id: string;
  model_id: number;
  serial_number: string;
  needs_manual_sn?: boolean;
}

async function authHeader(token: string) {
  return { Authorization: `Bearer ${token}` };
}

export async function getFirstActiveAircraftModelId(
  request: APIRequestContext,
  token: string,
): Promise<number> {
  const res = await request.get(`${API_BASE}/aircraft-models?is_active=true`, {
    headers: await authHeader(token),
  });
  if (!res.ok()) {
    throw new Error(`Failed to list aircraft models: ${res.status()} ${await res.text()}`);
  }
  const models = (await res.json()) as AircraftModel[];
  const active = models.find((m) => m.is_active);
  if (!active) {
    throw new Error(
      'No active aircraft models found. Seed at least one via the UI before running e2e tests.',
    );
  }
  return active.id;
}

/**
 * Create a test aircraft via the API.
 *
 * The backend's CreateAircraftRequest does NOT expose `needs_manual_sn` — it
 * is only set internally by the QR-scan and batch-import paths. To produce a
 * "pending review" aircraft for the merge UI, we create the row normally and
 * then flip the flag via direct DB UPDATE. This requires the docker-compose
 * dev stack to be running locally; see e2e/README.md.
 */
export async function createTestAircraft(
  request: APIRequestContext,
  token: string,
  opts: CreateOpts,
): Promise<Aircraft> {
  const res = await request.post(`${API_BASE}/aircraft`, {
    headers: await authHeader(token),
    data: {
      serial_number: opts.serial_number,
      equipment_id: opts.equipment_id,
      model_id: opts.model_id,
    },
  });
  if (!res.ok()) {
    throw new Error(`Failed to create aircraft: ${res.status()} ${await res.text()}`);
  }
  const aircraft = (await res.json()) as Aircraft;
  if (opts.needs_manual_sn) {
    flagAircraftNeedsManualSn(aircraft.id);
  }
  return aircraft;
}

/**
 * Set needs_manual_sn = TRUE on an aircraft row directly in Postgres. There is
 * no API surface for this — see createTestAircraft for the reason. Runs via
 * `docker compose exec` against the dev `db` service. Will throw if Docker is
 * not on PATH or the stack is not up.
 */
export function flagAircraftNeedsManualSn(aircraftId: number): void {
  if (!Number.isInteger(aircraftId) || aircraftId <= 0) {
    throw new Error(`Invalid aircraft id: ${aircraftId}`);
  }
  runPsql(`UPDATE aircraft SET needs_manual_sn = TRUE WHERE id = ${aircraftId};`);
}

/**
 * Set an aircraft's airworthiness via the real PATCH endpoint.
 *
 * NOTE: a freshly API-created aircraft starts as `not_airworthy` — the POST
 * /aircraft handler explicitly inserts `'not_airworthy'`, overriding the
 * `'airworthy'` column default (an airframe is grounded until signed off).
 * Tests that need an airworthy starting point must establish it here. Uses the
 * documented endpoint (audited) rather than a raw UPDATE.
 */
export async function setAirworthiness(
  request: APIRequestContext,
  token: string,
  aircraftId: number,
  status: 'airworthy' | 'not_airworthy',
  comment: string,
): Promise<void> {
  const res = await request.patch(`${API_BASE}/aircraft/${aircraftId}/airworthiness`, {
    headers: await authHeader(token),
    data: { airworthiness_status: status, comment },
  });
  if (!res.ok()) {
    throw new Error(
      `Failed to set airworthiness to ${status}: ${res.status()} ${await res.text()}`,
    );
  }
}

/**
 * Best-effort cleanup. Tries the API DELETE (which only RETIRES the aircraft),
 * then falls back to a hard DELETE via psql so the test data does not linger
 * as "retired" rows polluting the fleet list across runs.
 */
export async function cleanupTestAircraft(
  request: APIRequestContext,
  token: string,
  aircraftId: number,
): Promise<void> {
  try {
    await request.delete(`${API_BASE}/aircraft/${aircraftId}`, {
      headers: await authHeader(token),
    });
  } catch {
    // ignore — fall through to hard delete
  }
  try {
    runPsql(`DELETE FROM aircraft WHERE id = ${aircraftId};`);
  } catch (err) {
    // Cleanup failures shouldn't fail the test run; just surface to the
    // console so leaked rows can be removed manually (see README).
    // eslint-disable-next-line no-console
    console.warn(`cleanupTestAircraft(${aircraftId}) failed:`, err);
  }
}

/**
 * Best-effort cleanup of child rows that reference an aircraft, run BEFORE the
 * aircraft hard-delete in cleanupTestAircraft.
 *
 * Discovery (migration state on 2026-05-29):
 *   - defect_log.aircraft_id -> aircraft(id) ON DELETE SET NULL, so the
 *     aircraft hard-delete is NOT blocked — but the defect row survives with
 *     aircraft_id = NULL, leaking orphaned rows across runs. We delete those
 *     rows here so the dev DB stays clean.
 *   - audit_log has NO FK to aircraft (record_id is a plain integer), so it
 *     never blocks the delete either. It is also immutable (a BEFORE DELETE
 *     trigger raises), and EASA traceability wants it retained — so we
 *     deliberately do NOT touch audit_log here.
 *
 * Same dev-only, try/catch + console.warn pattern as cleanupTestAircraft.
 */
export function cleanupAircraftChildRows(aircraftId: number): void {
  if (!Number.isInteger(aircraftId) || aircraftId <= 0) {
    return;
  }
  try {
    runPsql(`DELETE FROM defect_log WHERE aircraft_id = ${aircraftId};`);
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn(`cleanupAircraftChildRows(${aircraftId}) failed:`, err);
  }
}

interface TestModel {
  id: number;
  name: string;
}

interface TestSchedule {
  id: number;
  name: string;
}

interface TestTemplate {
  id: number;
  name: string;
}

/**
 * Create a per-org aircraft model via the API. Model name is unique per org
 * (case-insensitive `LOWER(name)` guard in handlers/aircraft_models.rs), so
 * callers must pass a Date.now()-suffixed name.
 */
export async function createTestModel(
  request: APIRequestContext,
  token: string,
  opts: { name: string },
): Promise<TestModel> {
  const res = await request.post(`${API_BASE}/aircraft-models`, {
    headers: await authHeader(token),
    data: { name: opts.name, manufacturer: 'E2E', category: 'custom', notes: 'e2e test model' },
  });
  if (!res.ok()) {
    throw new Error(`Failed to create aircraft model: ${res.status()} ${await res.text()}`);
  }
  return (await res.json()) as TestModel;
}

/**
 * Create a maintenance schedule against an aircraft via the API. Values match
 * the create_schedule handler rules (interval_value > 0; warning_threshold is
 * accepted as-is, default 0.80). calendar_days / 365 is the simplest valid
 * combination.
 *
 * Optionally links an instruction template via instruction_template_id
 * (required for the completion spec post-#111; the backend
 * CreateMaintenanceScheduleRequest accepts it and validate_instruction_template
 * requires the template to match the aircraft's model).
 */
export async function createTestSchedule(
  request: APIRequestContext,
  token: string,
  opts: { aircraft_id: number; name: string; instruction_template_id?: number },
): Promise<TestSchedule> {
  const res = await request.post(`${API_BASE}/maintenance-schedules`, {
    headers: await authHeader(token),
    data: {
      aircraft_id: opts.aircraft_id,
      name: opts.name,
      description: null,
      interval_type: 'calendar_days',
      interval_value: 365,
      warning_threshold: 0.8,
      is_critical: false,
      secondary_interval_type: null,
      secondary_interval_value: null,
      instruction_template_id: opts.instruction_template_id ?? null,
    },
  });
  if (!res.ok()) {
    throw new Error(`Failed to create maintenance schedule: ${res.status()} ${await res.text()}`);
  }
  return (await res.json()) as TestSchedule;
}

/**
 * Create an inspection instruction template for an aircraft model, with ONE
 * section holding ONE required item. Linked to a schedule via
 * instruction_template_id (post-#111); CompleteScheduleModal loads exactly that
 * template and renders its items — there is no model-based fallback.
 */
export async function createInstructionTemplateForModel(
  request: APIRequestContext,
  token: string,
  opts: { aircraft_model_id: number; name: string },
): Promise<TestTemplate> {
  const res = await request.post(`${API_BASE}/maintenance-instruction-templates`, {
    headers: await authHeader(token),
    data: {
      name: opts.name,
      aircraft_model_id: opts.aircraft_model_id,
      description: 'E2E inspection template',
      revision: '01',
      sections: [
        {
          section_name: 'E2E Section',
          section_order: 1,
          items: [
            {
              item_text: 'E2E required inspection item',
              is_required: true,
              item_order: 1,
            },
          ],
        },
      ],
    },
  });
  if (!res.ok()) {
    throw new Error(`Failed to create instruction template: ${res.status()} ${await res.text()}`);
  }
  return (await res.json()) as TestTemplate;
}

/**
 * Tear down the full maintenance-completion seed in FK-safe (children-first)
 * order. Established from \d on 2026-05-29:
 *   - defect_log.aircraft_id -> aircraft  SET NULL   (orphans; clear first)
 *   - completions.checklist_item_id -> items  RESTRICT  <-- the real blocker:
 *       checklist items (and the template that CASCADE-deletes them) cannot be
 *       removed while a completion references them, so completions go first.
 *   - completions.maintenance_id -> maintenance  CASCADE
 *   - maintenance.aircraft_id / schedule_id / instruction_template_id  SET NULL
 *   - items.template_id -> templates  CASCADE
 *   - schedules.aircraft_id -> aircraft  CASCADE (drafts CASCADE off schedule)
 *   - templates.aircraft_model_id -> aircraft_models  SET NULL
 *   - aircraft.model_id -> aircraft_models  SET NULL
 * audit_log is deliberately never touched (immutable + EASA-retained).
 * Every delete is best-effort: a failure logs a warning and continues.
 */
export async function cleanupMaintenanceFlow(
  request: APIRequestContext,
  token: string,
  ids: { aircraftId: number; scheduleId: number; templateId: number; modelId: number },
): Promise<void> {
  const { aircraftId, scheduleId, templateId, modelId } = ids;

  // 1. defect_log (incl. the NOK auto-defect) — SET NULL would orphan it.
  if (aircraftId) cleanupAircraftChildRows(aircraftId);

  const steps: Array<[string, string]> = [
    // 2. completions — remove the RESTRICT blocker before items/template die.
    [
      'completions',
      `DELETE FROM maintenance_checklist_completions WHERE checklist_item_id IN ` +
        `(SELECT id FROM maintenance_checklist_items WHERE template_id = ${templateId});`,
    ],
    // 3. maintenance records for this aircraft (CASCADEs any stray completions).
    ['maintenance', `DELETE FROM maintenance WHERE aircraft_id = ${aircraftId};`],
    // 4. schedule (CASCADEs maintenance_drafts).
    ['schedule', `DELETE FROM maintenance_schedules WHERE id = ${scheduleId};`],
    // 5. template (CASCADEs checklist items, now unblocked).
    ['template', `DELETE FROM maintenance_instruction_templates WHERE id = ${templateId};`],
  ];
  for (const [label, sql] of steps) {
    try {
      runPsql(sql);
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn(`cleanupMaintenanceFlow step '${label}' failed:`, err);
    }
  }

  // 6. aircraft (API retire + hard delete).
  if (aircraftId) await cleanupTestAircraft(request, token, aircraftId);

  // 7. model — last, once nothing references it.
  try {
    if (modelId) runPsql(`DELETE FROM aircraft_models WHERE id = ${modelId};`);
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn(`cleanupMaintenanceFlow step 'model' failed:`, err);
  }
}

function runPsql(sql: string): void {
  // execFileSync avoids shell-quoting issues for the SQL string. The compose
  // file is at the repo root, one level up from frontend/, which is where the
  // tests run from.
  const composeFile = process.env.E2E_COMPOSE_FILE ?? '../docker-compose.dev.yml';
  execFileSync(
    'docker',
    ['compose', '-f', composeFile, 'exec', '-T', 'db', 'psql', '-U', 'dronefleet', '-d', 'dronefleet', '-c', sql],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );
}
