// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { test, expect } from './fixtures/auth';
import {
  createTestAircraft,
  cleanupTestAircraft,
  cleanupAircraftChildRows,
  getFirstActiveAircraftModelId,
} from './fixtures/test-data';

/**
 * End-to-end coverage of the defect-reporting flow on the aircraft workspace
 * Maintenance tab. Exercises:
 *   - workspace Maintenance tab "Open Defects" section (empty -> populated)
 *   - AddDefectModal date / 3-letter code / description validation + submit
 *   - POST /api/v1/defects (audited)
 *   - TanStack Query refetch reflecting the new open defect + count badge
 *
 * Uses its OWN bare test aircraft so it is independent of the airworthiness
 * spec. The created defect references this aircraft, so afterAll deletes the
 * defect_log child row before the aircraft hard-delete (defect_log FK is
 * ON DELETE SET NULL, which would otherwise leak an orphaned row).
 *
 * Prerequisite: docker-compose dev stack must be running. See e2e/README.md.
 */
test.describe('Defect reporting', () => {
  let timestamp: number;
  let aircraftId: number;

  test.beforeAll(async ({ request, adminToken }) => {
    timestamp = Date.now();
    const modelId = await getFirstActiveAircraftModelId(request, adminToken);

    const aircraft = await createTestAircraft(request, adminToken, {
      equipment_id: `UAV-e2e-defect-${timestamp}`,
      model_id: modelId,
      serial_number: `E2EDEF${timestamp}`,
    });
    aircraftId = aircraft.id;
  });

  test.afterAll(async ({ request, adminToken }) => {
    if (aircraftId) {
      cleanupAircraftChildRows(aircraftId);
      await cleanupTestAircraft(request, adminToken, aircraftId);
    }
  });

  test('admin can report a defect against an aircraft', async ({ adminPage }) => {
    await adminPage.goto(`/aircraft/${aircraftId}?tab=maintenance`);

    // Open Defects section starts empty for a fresh aircraft.
    await expect(adminPage.getByText('No open defects.')).toBeVisible();

    // The workspace persistent header ALSO carries a "Report defect" quick
    // action, so scope the trigger to the Open Defects panel to stay
    // unambiguous. Both open the same AddDefectModal.
    const defectsPanel = adminPage
      .locator('section')
      .filter({ has: adminPage.getByRole('heading', { name: 'Open Defects' }) });
    await defectsPanel.getByRole('button', { name: /report defect/i }).click();

    const dialog = adminPage.getByRole('dialog');
    await expect(dialog.getByRole('heading', { name: /report defect/i })).toBeVisible();

    // Defect date defaults to today (valid) — leave it as-is.
    // Reported-by strips non-letters, uppercases, max 3, requires exactly 3
    // ASCII letters. pressSequentially plays nicely with the transforming
    // controlled input.
    const reportedBy = dialog.locator('#defect-reported-by');
    await reportedBy.click();
    await reportedBy.pressSequentially('TST');

    await dialog
      .locator('#defect-description')
      .fill('E2E reported defect — cracked landing leg');

    // Report Defect submit is disabled until date + code + description are all
    // valid — Playwright auto-waits for it to become enabled.
    await dialog.getByRole('button', { name: /report defect/i }).click();

    // Modal closes and the detail query refetches: the empty-state disappears
    // and the new defect row shows up.
    await expect(adminPage.getByText('No open defects.')).toHaveCount(0);
    await expect(adminPage.getByText(/cracked landing leg/)).toBeVisible();

    // The Open Defects panel renders a count badge of 1.
    await expect(defectsPanel.getByText('1', { exact: true })).toBeVisible();
  });
});
