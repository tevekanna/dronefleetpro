// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { create } from 'zustand';

interface MaintenanceBannerState {
  dismissed: boolean;
  dismiss: () => void;
  reset: () => void;
}

// Session-scoped only — no persistence. Banner re-appears on reload/next login.
export const useMaintenanceBannerStore = create<MaintenanceBannerState>((set) => ({
  dismissed: false,
  dismiss: () => set({ dismissed: true }),
  reset: () => set({ dismissed: false }),
}));
