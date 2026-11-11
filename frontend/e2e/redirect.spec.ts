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
 * Pins the redirect contract from PR #140: the legacy per-aircraft maintenance
 * URL /maintenance/aircraft/:id must land on the workspace Maintenance tab at
 * /aircraft/:id?tab=maintenance. The Maintenance Hub row click and old
 * bookmarks rely on this.
 *
 * Prerequisite: docker-compose dev stack must be running. See e2e/README.md.
 */
test.describe('Legacy maintenance URL redirect', () => {
  let timestamp: number;
  let aircraftId: number;

  test.beforeAll(async ({ request, adminToken }) => {
    timestamp = Date.now();
    const modelId = await getFirstActiveAircraftModelId(request, adminToken);
    const aircraft = await createTestAircraft(request, adminToken, {
      equipment_id: `UAV-e2e-redir-${timestamp}`,
      model_id: modelId,
      serial_number: `E2EREDIR${timestamp}`,
    });
    aircraftId = aircraft.id;
  });

  test.afterAll(async ({ request, adminToken }) => {
    if (aircraftId) {
      cleanupAircraftChildRows(aircraftId);
      await cleanupTestAircraft(request, adminToken, aircraftId);
    }
  });

  test('old /maintenance/aircraft/:id lands on the workspace Maintenance tab', async ({
    adminPage,
  }) => {
    await adminPage.goto(`/maintenance/aircraft/${aircraftId}`);
    await expect(adminPage).toHaveURL(
      new RegExp(`/aircraft/${aircraftId}\\?tab=maintenance$`),
    );
  });
});
