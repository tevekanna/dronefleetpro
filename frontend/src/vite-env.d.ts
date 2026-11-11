// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

/// <reference types="vite/client" />

declare module '*.module.css' {
  const classes: { readonly [key: string]: string };
  export default classes;
}

interface ImportMetaEnv {
  /** Build-time override of the source-code link; see src/sourceCode.ts. */
  readonly VITE_SOURCE_CODE_URL?: string;
}

declare const __APP_VERSION__: string;
declare const __BUILD_TIMESTAMP__: string;
