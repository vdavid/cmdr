import org.jetbrains.intellij.platform.gradle.TestFrameworkType
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("jvm") version "2.4.10"
    id("org.jetbrains.intellij.platform") version "2.18.1"
}

group = "com.getcmdr"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

/** Expands a leading `~` so `gradle.properties` can name a path under the home dir. */
fun homePath(property: String): java.io.File {
    val raw = providers.gradleProperty(property).get()
    val expanded = if (raw.startsWith("~/")) System.getProperty("user.home") + raw.removePrefix("~") else raw
    return file(expanded)
}

val sveltePlugin = homePath("cmdrSveltePluginPath")

dependencies {
    // `BasePlatformTestCase` is a `junit.framework.TestCase`, and the platform test framework doesn't put JUnit on the
    // compile classpath itself. Without this, every test class fails with "Cannot access 'junit.framework.TestCase'".
    testImplementation("junit:junit:4.13.2")

    intellijPlatform {
        // A local install: no IDE download, and tier 2 shows exactly what David sees when he reads code.
        local(homePath("cmdrIdePath"))

        // Every feature walks real PSI. JavaScript backs the i18n features, Markdown backs the commit links, and
        // JSON is what a message key navigates *into*. All three are bundled, so they cost nothing at runtime.
        bundledPlugin("JavaScript")
        bundledPlugin("org.intellij.plugins.markdown")
        bundledPlugin("com.intellij.modules.json")

        // Optional: only present when the IDE has the Svelte plugin installed. The Svelte tests skip themselves
        // when it's absent, so a fresh clone still builds green.
        if (sveltePlugin.isDirectory) {
            localPlugin(sveltePlugin)
        }

        testFramework(TestFrameworkType.Platform)
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_25
        // Stay on the Kotlin API the platform bundles rather than the compiler's own latest; plugins that compile
        // against a newer stdlib API break at load time on the IDE's older runtime.
        apiVersion = org.jetbrains.kotlin.gradle.dsl.KotlinVersion.KOTLIN_2_2
        languageVersion = org.jetbrains.kotlin.gradle.dsl.KotlinVersion.KOTLIN_2_2
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_25
    targetCompatibility = JavaVersion.VERSION_25
}

intellijPlatform {
    pluginConfiguration {
        ideaVersion {
            sinceBuild = "262"
            // Deliberately open. We target an EAP, and a pinned upper bound means the plugin silently stops loading
            // at the next IDE upgrade. A plugin that breaks loudly beats one that disappears.
            untilBuild = provider { null }
        }
    }

    // Never published, so there's nothing to verify against a marketplace compatibility range.
    buildSearchableOptions = false
}

val sandboxProject = layout.projectDirectory.dir("sandbox-project").asFile

/** Where the marker lands inside the fixture project. Must match `CmdrProjectService.CONFIG_PATH`. */
val markerRelativePath = "tools/intellij-plugin/cmdr-plugin.json"
val markerFile = layout.projectDirectory.file("cmdr-plugin.json").asFile

/**
 * Marks `sandbox-project/` trusted before the sandbox IDE starts, and copies the real `cmdr-plugin.json` into it so
 * the plugin recognizes it as a Cmdr checkout.
 *
 * Without the trust seeding, a modal "Trust and Open Project?" dialog is the only thing the tier 2 screenshot ever
 * captures. Without the marker, every feature correctly does nothing, which looks exactly like a broken plugin. The
 * marker is copied rather than committed so the fixture can't drift from the config the repo actually ships.
 */
val seedIdeSandbox = tasks.register("seedIdeSandbox") {
    // Every value the action touches is captured into a local first. A task action that reaches a script-level `val`
    // holds a reference to the build script itself, which Gradle's configuration cache refuses to serialize, and the
    // whole of `runIde` fails to configure.
    val configDirectory = tasks.prepareSandbox.flatMap { it.sandboxConfigDirectory }
    val projectPath = sandboxProject.absolutePath
    val markerSource = markerFile
    val markerTarget = sandboxProject.resolve(markerRelativePath)
    outputs.upToDateWhen { false }
    doLast {
        markerTarget.apply { parentFile.mkdirs() }.writeText(markerSource.readText())

        val options = configDirectory.get().asFile.resolve("options").apply { mkdirs() }
        options.resolve("trusted-paths.xml").writeText(
            """
            <application>
              <component name="Trusted.Paths">
                <option name="TRUSTED_PROJECT_PATHS">
                  <map>
                    <entry key="$projectPath" value="true" />
                  </map>
                </option>
              </component>
              <component name="Trusted.Paths.Settings">
                <option name="TRUSTED_PATHS">
                  <list>
                    <option value="$projectPath" />
                  </list>
                </option>
              </component>
            </application>
            """.trimIndent() + "\n",
        )
    }
}

tasks.runIde {
    // Tier 2: land on the fixture with nothing to click, so the screenshot recipe in `DETAILS.md` is one shot. The
    // sandbox has its own config and plugin dirs, so it can never touch the IDE David has running.
    //
    // The second argument is what opens the file. Seeding the project's `.idea/workspace.xml` looks like the tidier
    // way and does not work: 2026.2 leaves the seeded file untouched on disk and still opens an empty editor.
    dependsOn(seedIdeSandbox)
    // Locals again, for the same configuration-cache reason as `seedIdeSandbox`.
    val projectPath = sandboxProject.absolutePath
    // One file per feature, the last one focused: `sample.ts` for i18n folding, `CHANGELOG.md` for the commit links.
    val filePaths = listOf("CHANGELOG.md", "sample.ts").map { sandboxProject.resolve(it).absolutePath }
    argumentProviders.add(CommandLineArgumentProvider { listOf(projectPath) + filePaths })
}

tasks.test {
    // `BasePlatformTestCase` is a `junit.framework.TestCase`, so the JUnit 4 runner, not the platform's JUnit 5 one.
    useJUnit()
    // Where the spike reads a real repo file from. Two levels up from `tools/intellij-plugin/`.
    systemProperty("cmdr.repo.root", rootDir.parentFile.parentFile.absolutePath)
    testLogging {
        showStandardStreams = true
        events("passed", "failed", "skipped")
    }
}
