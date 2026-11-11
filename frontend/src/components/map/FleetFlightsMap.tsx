// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useMemo, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import L from 'leaflet';
// leafletSetup must be imported before the cluster plugin. The plugin does
// not import Leaflet itself: it extends the global `L` that Leaflet's UMD
// build registers on window, which is the same object as the `L` imported
// here. Importing in this order guarantees that global exists first.
import { addBaseLayers, markerIcon } from './leafletSetup';
import 'leaflet.markercluster';
import 'leaflet.markercluster/dist/MarkerCluster.css';
import 'leaflet.markercluster/dist/MarkerCluster.Default.css';
import type { FlightMapPoint } from '../../api/dashboard';
import { formatISODate } from '../../utils/date';

interface FleetFlightsMapProps {
  points: FlightMapPoint[];
  height?: string;
}

// Tooltip content is built from DOM nodes with textContent, never from HTML
// strings: the aircraft name is user-controlled data and must not be able to
// inject markup.
function buildTooltip(point: FlightMapPoint): HTMLElement {
  const node = document.createElement('div');
  const date = document.createElement('strong');
  date.textContent = formatISODate(point.flight_datetime) || '—';
  const link = document.createElement('span');
  link.style.color = 'var(--color-accent)';
  link.textContent = 'Open flight →';
  node.append(
    date,
    document.createElement('br'),
    document.createTextNode(point.aircraft_name ?? '—'),
    document.createElement('br'),
    link,
  );
  return node;
}

export function FleetFlightsMap({ points, height = '420px' }: FleetFlightsMapProps) {
  const navigate = useNavigate();
  // Kept in a ref, updated on every render, so that the marker effect does
  // not have to depend on the navigate function.
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;

  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<L.Map | null>(null);
  const clusterRef = useRef<L.MarkerClusterGroup | null>(null);

  const latlngs = useMemo<[number, number][]>(
    () => (points ?? []).map((p) => [p.lat, p.lon]),
    [points],
  );
  const hasPoints = latlngs.length > 0;

  // Initial centre for map creation; the marker effect fits the viewport to
  // every flight location straight afterwards.
  const initialCentreRef = useRef<[number, number] | null>(null);
  initialCentreRef.current = hasPoints ? latlngs[0] : null;

  // Create the map once the container exists and remove it on unmount. The
  // container is only rendered when there are points (the dashboard passes an
  // empty list while its query loads), so the effect is keyed on that flag
  // rather than running strictly once. map.remove() releases the container,
  // which keeps this safe under the React 18 StrictMode double mount.
  useEffect(() => {
    const container = containerRef.current;
    const centre = initialCentreRef.current;
    if (!hasPoints || !container || !centre) return;

    const map = L.map(container, { center: centre, zoom: 6 });
    addBaseLayers(map);
    const cluster = L.markerClusterGroup({ chunkedLoading: true }).addTo(map);
    mapRef.current = map;
    clusterRef.current = cluster;

    // Leaflet measures its container once; keep it in step with later
    // layout changes so tiles never render grey.
    const resizeObserver = new ResizeObserver(() => map.invalidateSize());
    resizeObserver.observe(container);

    return () => {
      resizeObserver.disconnect();
      map.remove();
      mapRef.current = null;
      clusterRef.current = null;
    };
  }, [hasPoints]);

  // One marker per flight location, rebuilt when the points change.
  useEffect(() => {
    const map = mapRef.current;
    const cluster = clusterRef.current;
    if (!map || !cluster || latlngs.length === 0) return;

    cluster.clearLayers();
    cluster.addLayers(
      (points ?? []).map((p) =>
        L.marker([p.lat, p.lon], { icon: markerIcon })
          .bindTooltip(buildTooltip(p), { direction: 'top', offset: [0, -36] })
          .on('click', () => navigateRef.current(`/flights/${p.id}`)),
      ),
    );

    if (latlngs.length > 1) {
      map.fitBounds(latlngs, { padding: [40, 40] });
    } else {
      map.setView(latlngs[0], 13);
    }
  }, [points, latlngs]);

  // The empty state and the map container carry different keys so that React
  // never reuses one element for the other: Leaflet leaves its classes and
  // inline styles behind on a container even after map.remove().
  if (!hasPoints) {
    return (
      <div key="empty" style={{
        height,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'var(--color-bg-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        color: 'var(--color-text-muted)',
        fontFamily: 'var(--font-display)',
      }}>
        No flight locations with GPS data yet
      </div>
    );
  }

  return <div key="map" ref={containerRef} style={{ height, width: '100%', borderRadius: '6px' }} />;
}
