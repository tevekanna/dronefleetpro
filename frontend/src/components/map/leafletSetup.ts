// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

// Shared Leaflet set-up for the in-house map components.
//
// The application talks to plain Leaflet (BSD-2-Clause) directly instead of
// going through a third-party React binding: the one used previously is
// under the Hippocratic License 2.1, which is incompatible with AGPL-3.0.
// Everything the map components have in common lives here:
// the marker icon, the base tile layers and the layer switcher.
import L from 'leaflet';
import markerIconUrl from 'leaflet/dist/images/marker-icon.png';
import markerIconRetinaUrl from 'leaflet/dist/images/marker-icon-2x.png';
import markerShadowUrl from 'leaflet/dist/images/marker-shadow.png';

// Leaflet works out its default marker image paths from its own stylesheet
// URL, which breaks once Vite has bundled and fingerprinted the assets.
// Point the default icon at the bundled images instead, so any marker created
// without an explicit icon still renders.
delete (L.Icon.Default.prototype as unknown as Record<string, unknown>)._getIconUrl;
L.Icon.Default.mergeOptions({
  iconUrl: markerIconUrl,
  iconRetinaUrl: markerIconRetinaUrl,
  shadowUrl: markerShadowUrl,
});

/** The single marker icon used by every map in the application. */
export const markerIcon = new L.Icon({
  iconUrl: markerIconUrl,
  iconRetinaUrl: markerIconRetinaUrl,
  shadowUrl: markerShadowUrl,
  iconSize: [25, 41],
  iconAnchor: [12, 41],
  popupAnchor: [1, -34],
  tooltipAnchor: [16, -28],
});

export interface BaseLayers {
  Map: L.TileLayer;
  Satellite: L.TileLayer;
}

/**
 * Build the base tile layers. A Leaflet layer can belong to one map only, so
 * every call returns fresh instances.
 */
export function createBaseLayers(): BaseLayers {
  return {
    Map: L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      attribution:
        '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
    }),
    Satellite: L.tileLayer(
      'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}',
      {
        attribution:
          'Tiles &copy; Esri — Sources: Esri, Maxar, Earthstar Geographics, and the GIS user community',
        maxZoom: 19,
      },
    ),
  };
}

/**
 * Add the base layers to a map, with "Map" shown by default, together with
 * the Map/Satellite switcher in the top-right corner.
 */
export function addBaseLayers(map: L.Map): void {
  const baseLayers = createBaseLayers();
  baseLayers.Map.addTo(map);
  L.control.layers({ ...baseLayers }, undefined, { position: 'topright' }).addTo(map);
}
