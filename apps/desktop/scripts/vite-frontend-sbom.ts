/**
 * Vite plugin: write a CycloneDX SBOM of the npm packages whose code is in the built frontend.
 *
 * ## Why it reads the bundle, not `package.json`
 *
 * `package.json`'s dependencies/devDependencies split doesn't say what ships. The app is a
 * pre-built bundle, so the bundler takes code from either list: Svelte's runtime and SvelteKit's
 * client router come from devDependencies and are in the app, while most devDependencies (lint,
 * test, and build tools) aren't. `pnpm sbom --prod` would drop Svelte; without `--prod` it lists
 * every tool. `rollup-plugin-sbom` instead lists the packages behind the modules the bundle holds.
 *
 * ## What it adds by hand
 *
 * Packages whose content a plugin copies in without an import the bundler can trace. Today that's
 * `@iconify-json/lucide`: `unplugin-icons` compiles each used icon's SVG from that package's JSON
 * into a virtual `~icons/...` module. A new plugin that inlines a package's data belongs here too.
 *
 * ## When it runs
 *
 * Only when the release workflow's `sbom` job sets `CMDR_FRONTEND_SBOM=1` (see `vite.config.js`),
 * and only in the client build: SvelteKit's server build (used for prerendering) never ships.
 * The output is `build/sbom/frontend.cdx.json`. `saveTimestamp` is off so the file depends only on
 * the commit. How it's published: `docs/guides/releasing.md` § Provenance and SBOM attestations.
 */
// `@cyclonedx/cyclonedx-library` must be the exact version `rollup-plugin-sbom` pins: `Component` has
// private fields, so two copies make `bom.components.add()` a type error. Move them together.
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { Enums, Models } from '@cyclonedx/cyclonedx-library'
import sbom from 'rollup-plugin-sbom'
import type { Plugin } from 'vite'

/** Packages whose content ships without an import the bundler can see. */
const INLINED_PACKAGES = ['@iconify-json/lucide']

const require = createRequire(import.meta.url)

interface PackageJson {
  name: string
  version: string
  license?: string
}

/** A CycloneDX component for an installed package, in the same shape the plugin emits. */
function componentFor(packageName: string): Models.Component {
  const pkg = JSON.parse(readFileSync(require.resolve(`${packageName}/package.json`), 'utf8')) as PackageJson
  const [group, name] = pkg.name.startsWith('@') ? pkg.name.split('/') : [undefined, pkg.name]
  const purl = `pkg:npm/${group ? `${encodeURIComponent(group)}/` : ''}${name}@${pkg.version}`
  const licenses = new Models.LicenseRepository()
  if (pkg.license) {
    licenses.add(new Models.SpdxLicense(pkg.license, { acknowledgement: Enums.LicenseAcknowledgement.Declared }))
  }
  return new Models.Component(Enums.ComponentType.Library, name, {
    group,
    version: pkg.version,
    purl,
    bomRef: purl,
    licenses,
  })
}

export function frontendSbom(): Plugin {
  const plugin = sbom({
    specVersion: '1.5',
    outDir: 'sbom',
    outFilename: 'frontend.cdx',
    outFormats: ['json'],
    saveTimestamp: false,
    includeWellKnown: false,
    afterCollect(bom) {
      for (const packageName of INLINED_PACKAGES) {
        const component = componentFor(packageName)
        bom.components.add(component)
        bom.metadata.component?.dependencies.add(component.bomRef)
      }
    },
  }) as Plugin
  return { ...plugin, apply: 'build', applyToEnvironment: (environment) => environment.name === 'client' }
}
