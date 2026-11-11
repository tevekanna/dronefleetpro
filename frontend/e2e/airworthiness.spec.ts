// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { test, expect } from './fixtures/auth';
import {
  createTestAircraft,
  cleanupTestAircraft,
  cleanupAircraftChildRows,
  getFirstActiveAircraftModelId,
  setAirworthiness,
} from './fixtures/test-data';

/**
 * End-to-end coverage of the airworthiness status change flow on the aircraft
 * workspace. Exercises:
 *   - aircraft workspace persistent header airworthiness pill
 *   - AirworthinessModal radio + reason + Save Status
 *   - PATCH /api/v1/aircraft/:id/airworthiness (audited with the reason)
 *   - TanStack Query refetch reflecting the new status + reason
 *
 * NOTE: an API-created aircraft starts as 'not_airworthy' (the POST /aircraft
 * handler overrides the 'airworthy' column default — a new airframe is grounded
 * until signed off). To exercise the airworthy -> not_airworthy grounding flow,
 * beforeAll first clears it to 'airworthy' via the real PATCH endpoint.
 *
 * Prerequisite: docker-compose dev stack must be running. See e2e/README.md.
 */
test.describe('Airworthiness status change', () => {
  let timestamp: number;
  let aircraftId: number;

  test.beforeAll(async ({ request, adminToken }) => {
    timestamp = Date.now();
    const modelId = await getFirstActiveAircraftModelId(request, adminToken);

    const aircraft = await createTestAircraft(request, adminToken, {
      equipment_id: `UAV-e2e-aw-${timestamp}`,
      model_id: modelId,
      serial_number: `E2EAW${timestamp}`,
    });
    aircraftId = aircraft.id;

    // Aircraft is created grounded; sign it off so the UI test can ground it.
    await setAirworthiness(request, adminToken, aircraftId, 'airworthy', 'E2E precondition: signed off');
  });

  test.afterAll(async ({ request, adminToken }) => {
    if (aircraftId) {
      // The airworthiness change writes an audit_log row referencing the
      // aircraft (no defect rows here), but clean child rows defensively
      // before the hard delete regardless.
      cleanupAircraftChildRows(aircraftId);
      await cleanupTestAircraft(request, adminToken, aircraftId);
    }
  });

  test('admin can ground an aircraft (airworthy -> not_airworthy)', async ({ adminPage }) => {
    await adminPage.goto(`/aircraft/${aircraftId}?tab=maintenance`);

    // Header pill starts at AIRWORTHY (migration 039 default). exact:true so it
    // never matches the longer 'NOT AIRWORTHY' label.
    await expect(adminPage.getByText('AIRWORTHY', { exact: true })).toBeVisible();

    await adminPage.getByRole('button', { name: /change status/i }).click();

    const dialog = adminPage.getByRole('dialog');
    await expect(
      dialog.getByRole('heading', { name: /change airworthiness status/i }),
    ).toBeVisible();

    await dialog.locator('input[value="not_airworthy"]').check();
    await dialog.locator('#airworthiness-comment').fill('E2E grounding test');

    // Save Status is disabled until the status actually changed AND the comment
    // is >=5 chars — Playwright auto-waits for it to become enabled.
    await dialog.getByRole('button', { name: /save status/i }).click();

    // Modal closes and the detail query refetches. Pill flips to NOT AIRWORTHY
    // (exact:true so 'AIRWORTHY' substring doesn't match) and the reason — read
    // back from the audit-log diff envelope — is rendered.
    await expect(adminPage.getByText('NOT AIRWORTHY', { exact: true })).toBeVisible();
    await expect(adminPage.getByText(/E2E grounding test/)).toBeVisible();
  });
});
