// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import jsPDF from 'jspdf';
import autoTable from 'jspdf-autotable';
import type { ChecklistFull } from '../api/checklists';

const TYPE_LABEL: Record<string, string> = {
  normal_ops: 'Normal Operations',
  qrh: 'Quick Reference Handbook',
};

/// A5 portrait export matching the agreed reference layout: large
/// model-name title on the first page, dark section header strips with
/// white text, three-column item rows (label / value / responsibility).
/// Sections flagged start_on_new_page force pdf.addPage(); other sections
/// are paginated automatically by autoTable.
export function exportChecklistPdf(checklist: ChecklistFull, modelName: string): void {
  const pdf = new jsPDF({ unit: 'mm', format: 'a5', orientation: 'portrait' });
  const pageWidth = pdf.internal.pageSize.getWidth();
  const pageHeight = pdf.internal.pageSize.getHeight();
  const margin = 10;
  const contentWidth = pageWidth - margin * 2;

  // ---- First-page title ----
  pdf.setFontSize(18).setFont('helvetica', 'bold');
  pdf.text(modelName, pageWidth / 2, margin + 8, { align: 'center' });
  pdf.setFontSize(13).setFont('helvetica', 'normal');
  pdf.text(
    TYPE_LABEL[checklist.type] ?? checklist.type,
    pageWidth / 2,
    margin + 15,
    { align: 'center' },
  );

  pdf.setFontSize(9).setTextColor(100);
  const revLine = `Rev ${checklist.revision}${
    checklist.revision_date ? ` · ${checklist.revision_date}` : ''
  }`;
  pdf.text(revLine, pageWidth / 2, margin + 21, { align: 'center' });
  pdf.setTextColor(0);

  pdf.setDrawColor(0).setLineWidth(0.3);
  pdf.line(margin, margin + 25, pageWidth - margin, margin + 25);

  let cursorY = margin + 30;

  // ---- Sections ----
  checklist.sections.forEach((section, idx) => {
    const headerStripHeight = 7;

    // Honour explicit page-break flag (skip on first section so we don't
    // waste page 1 on a blank-then-section layout).
    if (section.start_on_new_page && idx > 0) {
      pdf.addPage();
      cursorY = margin;
    } else if (cursorY > pageHeight - 25) {
      // Cheap guard against starting a section flush against the bottom
      // edge. autoTable handles in-table pagination from there.
      pdf.addPage();
      cursorY = margin;
    }

    // Section header strip — filled dark rectangle with white text.
    pdf.setFillColor(30, 30, 30);
    pdf.rect(margin, cursorY, contentWidth, headerStripHeight, 'F');
    pdf
      .setTextColor(255)
      .setFont('helvetica', 'bold')
      .setFontSize(10);
    pdf.text(
      section.title.toUpperCase(),
      margin + 2,
      cursorY + headerStripHeight - 2,
    );
    pdf.setTextColor(0).setFont('helvetica', 'normal');
    cursorY += headerStripHeight + 2;

    if (section.items.length === 0) {
      pdf.setFontSize(9).setTextColor(150);
      pdf.text('(no items)', margin + 2, cursorY + 4);
      pdf.setTextColor(0);
      cursorY += 8;
      return;
    }

    autoTable(pdf, {
      startY: cursorY,
      head: [],
      body: section.items.map((item) => [
        item.label,
        item.value,
        `- ${item.responsibility}`,
      ]),
      styles: {
        fontSize: 9,
        cellPadding: { top: 1.5, right: 2, bottom: 1.5, left: 2 },
      },
      columnStyles: {
        0: { cellWidth: contentWidth * 0.45 },
        1: { cellWidth: contentWidth * 0.35, fontStyle: 'bold' },
        2: {
          cellWidth: contentWidth * 0.2,
          halign: 'right',
          textColor: [100, 100, 100],
        },
      },
      theme: 'plain',
      // Light row separator drawn after each cell of every row except the last.
      didDrawCell: (data) => {
        if (
          data.section === 'body' &&
          data.row.index < section.items.length - 1
        ) {
          pdf.setDrawColor(220).setLineWidth(0.1);
          pdf.line(
            data.cell.x,
            data.cell.y + data.cell.height,
            data.cell.x + data.cell.width,
            data.cell.y + data.cell.height,
          );
        }
      },
      margin: { left: margin, right: margin },
    });

    cursorY =
      (pdf as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable
        ?.finalY ?? cursorY;
    cursorY += 5;
  });

  // ---- Per-page footer ----
  const pageCount = pdf.getNumberOfPages();
  const footerLeft = `${modelName} — ${
    TYPE_LABEL[checklist.type] ?? checklist.type
  } — Rev ${checklist.revision}`;
  for (let i = 1; i <= pageCount; i++) {
    pdf.setPage(i);
    pdf.setFontSize(7).setTextColor(120);
    pdf.text(footerLeft, margin, pageHeight - 5);
    pdf.text(
      `Page ${i} of ${pageCount}`,
      pageWidth - margin,
      pageHeight - 5,
      { align: 'right' },
    );
    pdf.setTextColor(0);
  }

  const safeModel = modelName.replace(/[^a-zA-Z0-9_-]/g, '_');
  const safeRev = checklist.revision.replace(/[^a-zA-Z0-9_-]/g, '_');
  pdf.save(`${safeModel}_${checklist.type}_rev${safeRev}.pdf`);
}
