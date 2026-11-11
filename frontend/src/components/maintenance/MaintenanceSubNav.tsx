// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { NavLink, useSearchParams } from 'react-router-dom';
import styles from './MaintenanceSubNav.module.css';

type TabScope = 'aircraft' | 'none';

type SubNavTab = {
  label: string;
  path: string;
  scope: TabScope;
  end?: boolean;
};

const TABS: SubNavTab[] = [
  { label: 'Hub', path: '/maintenance', scope: 'none', end: true },
  { label: 'Records & Defects', path: '/maintenance-records', scope: 'aircraft' },
  { label: 'Schedules', path: '/maintenance-schedules', scope: 'aircraft' },
];

export function MaintenanceSubNav() {
  // Aircraft-scoped tabs carry the current ?aircraft= context so it survives
  // moving between Records & Defects and Schedules. Model-scoped management
  // now lives on the aircraft workspace "Model & checklists" tab, so no model
  // forwarding is needed here.
  const [searchParams] = useSearchParams();
  const urlAircraft = searchParams.get('aircraft');
  const ctxAircraft = urlAircraft ? Number(urlAircraft) : undefined;

  function buildTo(path: string, scope: TabScope): string {
    if (scope === 'none' || ctxAircraft == null) return path;
    return `${path}?aircraft=${ctxAircraft}`;
  }

  return (
    <nav className={styles.subNav}>
      {TABS.map(({ label, path, scope, end }) => (
        <NavLink
          key={path}
          to={buildTo(path, scope)}
          end={end}
          className={({ isActive }) =>
            `${styles.tab} ${isActive ? styles.tabActive : ''}`
          }
        >
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
