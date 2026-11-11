// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! EASA compliance validation hooks.
//!
//! Reserved for backend defence-in-depth checks against EASA SAIL I/II
//! requirements. The previous validators (validate_easa_category,
//! validate_mission_type, validate_maint_category, validate_maint_type,
//! validate_user_code) were never wired up — frontend Zod schemas and
//! database CHECK constraints carry the validation today. The module
//! is kept so a future wiring of backend validators has an obvious home.
