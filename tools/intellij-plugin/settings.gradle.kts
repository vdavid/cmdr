// Standalone Gradle build. Deliberately not part of the pnpm workspace (`packages: apps/*`) or the Cargo workspace:
// `pnpm check` never sees it and CI never drags a JVM in.
rootProject.name = "cmdr-idea-plugin"
