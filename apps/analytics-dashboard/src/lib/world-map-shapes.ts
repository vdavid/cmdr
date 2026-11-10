/**
 * The world map's geometry: the 110m `visionscarto-world-atlas` countries, projected once with Equal Earth
 * (equal-area, so a big country doesn't overstate its count) into SVG path strings. The geo libraries and the
 * ~125 KB atlas load lazily on first use, and the result is cached for the page's lifetime, so the map
 * costs nothing until it's on screen and is projected exactly once.
 */
import { antarcticaMapId, countryForMapId } from './country-geo.js'

export interface MapShape {
  /** The atlas feature id (ISO-3166 numeric, or negative for the three shapes without one). */
  id: string
  /** The alpha-2 code the shape stands for. */
  country: string
  /** The SVG path, in the `width` × `height` viewBox. */
  d: string
}

export interface MapGeometry {
  shapes: MapShape[]
  width: number
  height: number
}

/** The viewBox width; the height follows from the projection. */
const mapWidth = 960

let geometry: Promise<MapGeometry> | null = null

/** The projected map, loaded and computed on the first call and shared after. */
export function loadMapGeometry(): Promise<MapGeometry> {
  geometry ??= projectMap().catch((e: unknown) => {
    // Let a later mount retry rather than caching the failure.
    geometry = null
    throw e
  })
  return geometry
}

async function projectMap(): Promise<MapGeometry> {
  const [geo, topo, atlasModule] = await Promise.all([
    import('d3-geo'),
    import('topojson-client'),
    import('visionscarto-world-atlas/world/110m.json'),
  ])
  const topology = atlasModule.default as unknown as Parameters<typeof topo.feature>[0]
  const countries = topology.objects.countries as Parameters<typeof topo.feature>[1]
  const collection = topo.feature(topology, countries) as unknown as GeoJSON.FeatureCollection
  collection.features = collection.features.filter((f) => String(f.id) !== antarcticaMapId)

  const projection = geo.geoEqualEarth().fitWidth(mapWidth, collection)
  const path = geo.geoPath(projection).digits(1)
  const [, [, bottom]] = path.bounds(collection)

  const shapes: MapShape[] = []
  for (const f of collection.features) {
    const id = String(f.id)
    const country = countryForMapId(id)
    const d = path(f)
    if (country && d) shapes.push({ id, country, d })
  }
  return { shapes, width: mapWidth, height: Math.ceil(bottom) }
}
