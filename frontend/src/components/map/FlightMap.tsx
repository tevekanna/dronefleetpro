// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useMemo, useRef } from 'react';
import L from 'leaflet';
import { addBaseLayers, markerIcon } from './leafletSetup';

interface TrajectoryPoint {
  lat: number;
  lon: number;
  alt: number;
  t_sec: number;
}

interface FlightMapProps {
  trajectory: TrajectoryPoint[];
  homeLat?: number | null;
  homeLon?: number | null;
  height?: string;
}

// Popup content is built from DOM nodes with textContent, never from HTML
// strings, so no value can ever be interpreted as markup.
function buildPopup(title: string, point: [number, number]): HTMLElement {
  const node = document.createElement('div');
  const heading = document.createElement('strong');
  heading.textContent = title;
  node.append(
    heading,
    document.createElement('br'),
    document.createTextNode(`Lat: ${point[0].toFixed(5)}`),
    document.createElement('br'),
    document.createTextNode(`Lon: ${point[1].toFixed(5)}`),
  );
  return node;
}

export function FlightMap({ trajectory, homeLat, homeLon, height = '400px' }: FlightMapProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<L.Map | null>(null);

  const points = useMemo<[number, number][]>(
    () => (trajectory ?? []).map((p) => [p.lat, p.lon]),
    [trajectory],
  );
  const hasPoints = points.length > 0;

  // Initial centre for map creation, kept in a ref so that the creation
  // effect does not have to depend on the home position or the trajectory.
  const initialCentreRef = useRef<[number, number] | null>(null);
  initialCentreRef.current = hasPoints
    ? (homeLat && homeLon ? [homeLat, homeLon] : points[0])
    : null;

  // Create the map once the container exists and remove it on unmount. The
  // container is only rendered when there is a trajectory, so the effect is
  // keyed on that flag rather than running strictly once: a component that
  // first renders the empty state still gets its map when data arrives.
  // map.remove() releases the container, which keeps this safe under the
  // React 18 StrictMode double mount.
  useEffect(() => {
    const container = containerRef.current;
    const centre = initialCentreRef.current;
    if (!hasPoints || !container || !centre) return;

    const map = L.map(container, { center: centre, zoom: 15 });
    addBaseLayers(map);
    mapRef.current = map;

    // Leaflet measures its container once. Inside the import modal, or when
    // the layout changes, the size settles later; without this the map shows
    // grey, unloaded tiles.
    const resizeObserver = new ResizeObserver(() => map.invalidateSize());
    resizeObserver.observe(container);

    return () => {
      resizeObserver.disconnect();
      map.remove();
      mapRef.current = null;
    };
  }, [hasPoints]);

  // Draw the flight track and its markers, and rebuild them when the
  // trajectory changes.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || points.length === 0) return;

    const startPoint = points[0];
    const endPoint = points[points.length - 1];
    const group = L.layerGroup().addTo(map);

    L.polyline(points, { color: '#0ea5e9', weight: 3, opacity: 0.8 }).addTo(group);
    L.marker(startPoint, { icon: markerIcon })
      .bindPopup(buildPopup('Takeoff', startPoint))
      .addTo(group);
    if (startPoint[0] !== endPoint[0] || startPoint[1] !== endPoint[1]) {
      L.marker(endPoint, { icon: markerIcon })
        .bindPopup(buildPopup('Landing', endPoint))
        .addTo(group);
    }

    if (points.length > 1) {
      map.fitBounds(points, { padding: [20, 20] });
    } else {
      map.setView(startPoint, 15);
    }

    return () => {
      group.clearLayers();
      group.remove();
    };
  }, [points]);

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
        No GPS trajectory data available
      </div>
    );
  }

  return <div key="map" ref={containerRef} style={{ height, width: '100%', borderRadius: '6px' }} />;
}
