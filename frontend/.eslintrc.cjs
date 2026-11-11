// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

// ESLint configuration (legacy eslintrc format — the default for ESLint 8.x).
//
// The load-bearing rule here is react-hooks/rules-of-hooks: hook-ordering
// violations (conditional hooks, hooks in loops or callbacks) are NOT caught
// by tsc or the Vite build, and this project has relied on manual review to
// catch them until now. exhaustive-deps is advisory (warn) — missing
// dependencies are usually bugs but each fix needs individual judgement.
module.exports = {
  root: true,
  env: { browser: true, es2022: true },
  ignorePatterns: ['dist', 'node_modules', 'e2e', 'test-results', 'playwright-report'],
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 2022,
    sourceType: 'module',
    ecmaFeatures: { jsx: true },
  },
  plugins: ['@typescript-eslint', 'react-hooks'],
  overrides: [
    {
      files: ['src/**/*.ts', 'src/**/*.tsx'],
      rules: {
        'react-hooks/rules-of-hooks': 'error',
        'react-hooks/exhaustive-deps': 'warn',
      },
    },
  ],
};
