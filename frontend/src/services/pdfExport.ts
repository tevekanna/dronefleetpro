// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import jsPDF from 'jspdf';
import autoTable from 'jspdf-autotable';
import type { FleetStatsOverview } from '../types/fleetStats';

function fmtDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h === 0) return `${m}m`;
  return `${h}h ${m.toString().padStart(2, '0')}m`;
}

function fmtDate(iso: string | null): string {
  return iso ? iso.slice(0, 10) : '—';
}

/**
 * Generate a client-side summary PDF of the fleet statistics overview.
 * Tables only — charts deliberately skipped (spec deferral).
 */
export async function exportFleetStatsSummary(overview: FleetStatsOverview): Promise<void> {
  const doc = new jsPDF({ unit: 'pt', format: 'a4' });
  const generatedAt = new Date().toISOString().slice(0, 10);

  // Header
  doc.setFontSize(18);
  doc.text('Fleet Statistics Summary', 40, 48);
  doc.setFontSize(10);
  doc.setTextColor(120);
  doc.text(`Generated ${generatedAt} · DroneFleetPro v${__APP_VERSION__}`, 40, 66);
  doc.setTextColor(0);

  // Top-level counts
  autoTable(doc, {
    startY: 90,
    head: [['Metric', 'Count']],
    body: [
      ['Batteries active', String(overview.total_batteries_active)],
      ['Batteries near EoL (≥80% nominal)', String(overview.total_batteries_near_eol)],
      ['Batteries past nominal EoL', String(overview.total_batteries_past_eol)],
      ['Controllers active', String(overview.total_controllers_active)],
      ['Controllers inactive >90 days', String(overview.total_controllers_inactivity_warning)],
    ],
    theme: 'striped',
    headStyles: { fillColor: [14, 165, 233] },
    styles: { fontSize: 10 },
  });

  // Battery models summary
  const modelsY = (doc as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable?.finalY ?? 200;
  doc.setFontSize(13);
  doc.text('Battery models', 40, modelsY + 30);
  autoTable(doc, {
    startY: modelsY + 40,
    head: [['Model', 'Count', 'Avg cycles', 'Min / Max cycles', 'Nominal', 'Avg discharge']],
    body: overview.battery_model_summary.map((m) => [
      m.model,
      String(m.battery_count),
      m.avg_cycle_count.toFixed(0),
      `${m.min_cycle_count} / ${m.max_cycle_count}`,
      m.nominal_max_cycles !== null
        ? `${m.nominal_max_cycles} ± ${m.cycle_uncertainty ?? 0}`
        : '—',
      m.avg_discharge_per_flight !== null
        ? `${m.avg_discharge_per_flight.toFixed(1)}%`
        : '—',
    ]),
    theme: 'striped',
    headStyles: { fillColor: [14, 165, 233] },
    styles: { fontSize: 9 },
  });

  // Top 5 batteries
  const batY = (doc as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable?.finalY ?? 400;
  doc.setFontSize(13);
  doc.text('Most used batteries (last 90 days)', 40, batY + 30);
  autoTable(doc, {
    startY: batY + 40,
    head: [['Equipment', 'Model', 'Flights', 'Flight time', 'Cycles', 'Last used']],
    body: overview.recently_flown_batteries_top5.map((b) => [
      b.equipment_id ?? b.serial_number,
      b.model ?? '—',
      String(b.total_flights),
      fmtDuration(b.total_flight_seconds),
      String(b.cycle_count),
      fmtDate(b.last_used),
    ]),
    theme: 'striped',
    headStyles: { fillColor: [14, 165, 233] },
    styles: { fontSize: 9 },
  });

  // Top 5 controllers
  const ctlY = (doc as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable?.finalY ?? 550;
  doc.setFontSize(13);
  doc.text('Most used controllers (last 90 days)', 40, ctlY + 30);
  autoTable(doc, {
    startY: ctlY + 40,
    head: [['Equipment', 'Model', 'Flights', 'Flight time', 'Last used']],
    body: overview.recently_flown_controllers_top5.map((c) => [
      c.equipment_id ?? c.serial_number,
      c.model ?? '—',
      String(c.total_flights),
      fmtDuration(c.total_flight_seconds),
      fmtDate(c.last_used),
    ]),
    theme: 'striped',
    headStyles: { fillColor: [14, 165, 233] },
    styles: { fontSize: 9 },
  });

  // Footer with page numbers
  const pages = doc.getNumberOfPages();
  for (let i = 1; i <= pages; i++) {
    doc.setPage(i);
    doc.setFontSize(8);
    doc.setTextColor(140);
    doc.text(
      `Page ${i} of ${pages}`,
      doc.internal.pageSize.getWidth() - 60,
      doc.internal.pageSize.getHeight() - 24,
    );
    doc.setTextColor(0);
  }

  doc.save(`fleet-stats-${generatedAt}.pdf`);
}
