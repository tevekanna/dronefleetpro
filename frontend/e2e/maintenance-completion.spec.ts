// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { test, expect } from './fixtures/auth';
import {
  createTestAircraft,
  createTestModel,
  createTestSchedule,
  createInstructionTemplateForModel,
  cleanupMaintenanceFlow,
} from './fixtures/test-data';

/**
 * End-to-end coverage of the perform -> complete maintenance-schedule flow,
 * including the NOK-checklist-item -> auto-defect automation. Exercises:
 *   - aircraft workspace Maintenance tab schedule row "Perform" button
 *   - CompleteScheduleModal: form fields + inspection checklist (NOK + note)
 *   - POST /maintenance-schedules/:id/complete (creates the maintenance record)
 *   - POST /maintenance/:id/completions (saves checklist + auto-creates defects)
 *   - TanStack Query refetch reflecting the new maintenance record AND defect
 *
 * This needs the heaviest seed so far: a dedicated model -> aircraft (on that
 * model) -> instruction template (on the model, one required item) -> schedule
 * (on the aircraft, explicitly LINKED to that template via
 * instruction_template_id). Post-#111 CompleteScheduleModal reads
 * schedule.instruction_template_id directly — there is no model-based fallback,
 * so the schedule must carry the link or no checklist renders. Torn down
 * FK-safe in afterAll.
 *
 * Discovery (2026-05-29): the auto-created defect carries aircraft_id (bound
 * from the maintenance record's aircraft_id in handlers/maintenance.rs
 * save_completions), so it surfaces in the aircraft's "Open Defects" panel —
 * we assert there rather than via GET /defects. Its description is
 * "[Checklist NOK] <item_text>: <note>".
 *
 * Prerequisite: docker-compose dev stack must be running. See e2e/README.md.
 */
test.describe('Maintenance schedule completion', () => {
  let timestamp: number;
  let modelId: number;
  let aircraftId: number;
  let scheduleId: number;
  let templateId: number;
  let scheduleName: string;

  test.beforeAll(async ({ request, adminToken }) => {
    timestamp = Date.now();

    // Dedicated model so the instruction template only matches THIS aircraft.
    const model = await createTestModel(request, adminToken, {
      name: `E2E Maint Model ${timestamp}`,
    });
    modelId = model.id;

    const aircraft = await createTestAircraft(request, adminToken, {
      equipment_id: `UAV-e2e-maint-${timestamp}`,
      model_id: modelId,
      serial_number: `E2EMAINT${timestamp}`,
    });
    aircraftId = aircraft.id;

    // Template must exist BEFORE the schedule so it can be linked at create
    // time (post-#111 the schedule carries instruction_template_id; there is
    // no model-based fallback in CompleteScheduleModal).
    const template = await createInstructionTemplateForModel(request, adminToken, {
      aircraft_model_id: modelId,
      name: `E2E Template ${timestamp}`,
    });
    templateId = template.id;

    scheduleName = `E2E Inspection ${timestamp}`;
    const schedule = await createTestSchedule(request, adminToken, {
      aircraft_id: aircraftId,
      name: scheduleName,
      instruction_template_id: templateId,
    });
    scheduleId = schedule.id;
  });

  test.afterAll(async ({ request, adminToken }) => {
    await cleanupMaintenanceFlow(request, adminToken, {
      aircraftId,
      scheduleId,
      templateId,
      modelId,
    });
  });

  test('admin completes a scheduled inspection and a NOK item auto-creates a defect', async ({
    adminPage,
  }) => {
    await adminPage.goto(`/aircraft/${aircraftId}?tab=maintenance`);

    // Find our schedule row (auto-seeded defaults may also be present) and
    // perform it.
    const scheduleRow = adminPage.locator('li').filter({ hasText: scheduleName });
    await expect(scheduleRow).toBeVisible();
    await scheduleRow.getByRole('button', { name: /^perform$/i }).click();

    // CompleteScheduleModal has NO role="dialog" — anchor on the heading and
    // the stable input ids instead.
    await expect(adminPage.getByRole('heading', { name: /complete:/i })).toBeVisible();

    // Leave Performed date (today) and Type (Test) at their defaults.
    await adminPage.locator('#complete-remark').fill('E2E scheduled inspection remark');
    await adminPage.locator('#complete-action-taken').fill('Inspected per E2E procedure');
    await adminPage.locator('#complete-sign-off').fill('TST');

    // Mark the one required checklist item NOK, then describe the fault. The
    // NOK note textarea only renders once the item status is 'nok'.
    await adminPage.getByRole('button', { name: 'NOK', exact: true }).click();
    await adminPage
      .getByPlaceholder(/describe the fault/i)
      .fill('E2E fault: cracked propeller blade');

    await adminPage.getByRole('button', { name: /log completion/i }).click();

    // Modal closes and the detail query refetches.
    // 1. Recent Maintenance now has a record (empty state gone).
    const historyPanel = adminPage
      .locator('section')
      .filter({ has: adminPage.getByRole('heading', { name: /recent maintenance/i }) });
    await expect(historyPanel.getByText('No maintenance records.')).toHaveCount(0);
    await expect(historyPanel.locator('li').first()).toBeVisible();

    // 2. The NOK item auto-created a defect carrying this aircraft_id, so it
    //    appears in the Open Defects panel (assertion path chosen per the
    //    discovery note above — the defect is aircraft-scoped, not orphaned).
    const defectsPanel = adminPage
      .locator('section')
      .filter({ has: adminPage.getByRole('heading', { name: 'Open Defects' }) });
    await expect(defectsPanel.getByText('No open defects.')).toHaveCount(0);
    await expect(defectsPanel.getByText(/Checklist NOK/)).toBeVisible();
  });
});
