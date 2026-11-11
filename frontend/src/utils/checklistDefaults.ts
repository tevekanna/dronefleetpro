// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { ChecklistSectionInput } from '../api/checklists';

/// Default starter sections offered by the "Default sections" choice in the
/// new-checklist modal. Items are intentionally empty — the operator fills
/// them in. Ordering follows the standard pre/take-off/landing/post-flight
/// flow.
export const DEFAULT_SECTIONS: ChecklistSectionInput[] = [
  { title: 'Preflight', sort_order: 1, start_on_new_page: false, items: [] },
  { title: 'Before Take Off', sort_order: 2, start_on_new_page: false, items: [] },
  { title: 'Before Landing', sort_order: 3, start_on_new_page: false, items: [] },
  { title: 'After Landing', sort_order: 4, start_on_new_page: false, items: [] },
];
