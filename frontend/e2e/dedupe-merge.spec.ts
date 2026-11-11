// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { test, expect } from './fixtures/auth';
import {
  createTestAircraft,
  cleanupTestAircraft,
  getFirstActiveAircraftModelId,
} from './fixtures/test-data';

/**
 * End-to-end coverage of the aircraft dedupe-merge flow. Exercises the full
 * stack:
 *   - Backend pending-review state (needs_manual_sn = TRUE)
 *   - FleetPage "Show pending review only" toggle
 *   - EditAircraftModal "Merge into existing aircraft" CTA
 *   - MergeAircraftModal pick → confirm → done flow
 *   - POST /api/v1/aircraft/:source_id/merge endpoint
 *
 * Prerequisite: docker-compose dev stack must be running. See e2e/README.md.
 */
test.describe('Aircraft dedupe merge flow', () => {
  let timestamp: number;
  let pendingAircraftId: number;
  let targetAircraftId: number;

  test.beforeAll(async ({ request, adminToken }) => {
    timestamp = Date.now();
    const modelId = await getFirstActiveAircraftModelId(request, adminToken);

    // Canonical target: active, verified SN.
    const target = await createTestAircraft(request, adminToken, {
      equipment_id: `UAV-e2e-target-${timestamp}`,
      model_id: modelId,
      serial_number: `E2ETARGET${timestamp}`,
    });
    targetAircraftId = target.id;

    // Pending source: same model, flagged needs_manual_sn (via direct DB
    // UPDATE — the API doesn't expose this field at create-time).
    const source = await createTestAircraft(request, adminToken, {
      equipment_id: `UAV-e2e-pending-${timestamp}`,
      model_id: modelId,
      serial_number: `E2EPENDING${timestamp}`,
      needs_manual_sn: true,
    });
    pendingAircraftId = source.id;
  });

  test.afterAll(async ({ request, adminToken }) => {
    // The merge endpoint hard-deletes the source on success. The target row
    // remains and must be cleaned up. On test failure the source may also
    // still exist — both paths handled by best-effort cleanup.
    if (targetAircraftId) {
      await cleanupTestAircraft(request, adminToken, targetAircraftId);
    }
    if (pendingAircraftId) {
      await cleanupTestAircraft(request, adminToken, pendingAircraftId);
    }
  });

  test('admin can merge a pending aircraft into a canonical one', async ({ adminPage }) => {
    await adminPage.goto('/fleet');

    // The "Show pending review only" toggle appears in the top-right of the
    // aircraft tab when at least one row has needs_manual_sn=TRUE.
    const pendingToggle = adminPage.getByRole('button', {
      name: /show pending review only/i,
    });
    await expect(pendingToggle).toBeVisible();

    // Clicking the toggle filters AND auto-expands every accordion group
    // (forceExpand={isFiltered} in FleetPage.tsx).
    await pendingToggle.click();

    const pendingEqId = `UAV-e2e-pending-${timestamp}`;
    const pendingRow = adminPage.locator('tr').filter({ hasText: pendingEqId });
    await expect(pendingRow).toBeVisible();

    // Per-row Edit button is plain text "Edit". stopPropagation is on the
    // onClick so the row navigation isn't triggered.
    await pendingRow.getByRole('button', { name: 'Edit' }).click();

    // EditAircraftModal: the "Merge into existing aircraft" CTA only renders
    // when the aircraft is flagged needs_manual_sn.
    const mergeCta = adminPage.getByRole('button', { name: /merge into existing aircraft/i });
    await expect(mergeCta).toBeVisible();
    await mergeCta.click();

    // MergeAircraftModal — 'pick' stage. The parent EditAircraftModal is
    // still in the DOM behind us (with its own Model and Status selects),
    // so we have to scope to the merge modal's form. The merge modal's
    // <form> contains the body text "Merge <source label> into another
    // aircraft of the same model" — a stable scoping hook.
    const mergeForm = adminPage.locator('form').filter({ hasText: /merge .* into another aircraft of the same model/i });
    await mergeForm.getByRole('combobox').selectOption({ value: String(targetAircraftId) });

    await adminPage.getByRole('button', { name: /^next$/i }).click();

    // 'confirm' stage: warning banner + Confirm Merge button.
    await expect(adminPage.getByText(/this cannot be undone/i)).toBeVisible();
    await adminPage.getByRole('button', { name: /confirm merge/i }).click();

    // 'done' stage: summary heading.
    await expect(adminPage.getByRole('heading', { name: /merge complete/i })).toBeVisible();

    // Dismiss — closes both the merge modal AND the parent EditAircraftModal.
    await adminPage.getByRole('button', { name: /^done$/i }).click();

    // Source row should be gone — backend hard-deleted the aircraft, and
    // TanStack Query invalidation refetches the list.
    await expect(adminPage.locator('tr').filter({ hasText: pendingEqId })).toHaveCount(0);

    // The merge succeeded — mark pending as cleaned so afterAll skips it.
    pendingAircraftId = 0;
  });
});
