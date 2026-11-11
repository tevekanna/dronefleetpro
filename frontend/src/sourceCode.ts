// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

// Where the source code of the running application can be obtained.
//
// DroneFleetPro is licensed under AGPL-3.0-only, which requires that every
// user interacting with it over a network is offered the corresponding
// source. Operators running a modified version must set VITE_SOURCE_CODE_URL
// at build time so that this link points at their own source.

const DEFAULT_SOURCE_CODE_URL = 'https://github.com/tevekanna/dronefleetpro';

const override = import.meta.env.VITE_SOURCE_CODE_URL?.trim();

export const SOURCE_CODE_URL: string = override || DEFAULT_SOURCE_CODE_URL;
