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

        // Both features walk real PSI. JavaScript backs feature 2 (i18n folding), Markdown backs feature 1
        // (changelog commit links). Both are bundled in IDEA Ultimate, so they cost nothing at runtime.
        bundledPlugin("JavaScript")
        bundledPlugin("org.intellij.plugins.markdown")

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

/**
 * Makes `runIde` land on the fixture with nothing to click: marks `sandbox-project/` trusted (otherwise a modal
 * "Trust and Open Project?" dialog is all the tier 2 screenshot ever captures) and pre-opens `Probe.svelte` in the
 * editor. Both files are regenerated every run, so editing them by hand in the sandbox IDE won't stick.
 */
val seedIdeSandbox = tasks.register("seedIdeSandbox") {
    val configDirectory = tasks.prepareSandbox.flatMap { it.sandboxConfigDirectory }
    val projectPath = sandboxProject.absolutePath
    outputs.upToDateWhen { false }
    doLast {
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

        val projectDir = "${'$'}PROJECT_DIR${'$'}"
        val ideaDirectory = File(projectPath, ".idea").apply { mkdirs() }
        ideaDirectory.resolve("workspace.xml").writeText(
            """
            <project version="4">
              <component name="FileEditorManager">
                <leaf>
                  <file pinned="false" current="true" current-in-tab="true">
                    <entry file="file://$projectDir/Probe.svelte">
                      <provider selected="true" editor-type-id="text-editor" />
                    </entry>
                  </file>
                  <file pinned="false" current="false" current-in-tab="false">
                    <entry file="file://$projectDir/probe.ts">
                      <provider selected="true" editor-type-id="text-editor" />
                    </entry>
                  </file>
                </leaf>
              </component>
            </project>
            """.trimIndent() + "\n",
        )
    }
}

tasks.runIde {
    // Tier 2: open the seeded project straight away, so the screenshot recipe in `DETAILS.md` needs no clicking.
    // The sandbox has its own config and plugin dirs, so it can never touch the IDE David has running.
    dependsOn(seedIdeSandbox)
    argumentProviders.add(
        CommandLineArgumentProvider {
            listOf(sandboxProject.absolutePath, sandboxProject.resolve("probe.ts").absolutePath)
        },
    )
}

tasks.test {
    // BasePlatformTestCase is JUnit 3 via JUnit 4's runner; the platform test framework brings its own.
    useJUnit()
    // The in-process platform wants a headless AWT and its own temp dirs.
    systemProperty("java.awt.headless", "true")
    systemProperty("idea.force.use.core.classloader", "true")
    // Where the spike reads a real repo file from. Two levels up from `tools/intellij-plugin/`.
    systemProperty("cmdr.repo.root", rootDir.parentFile.parentFile.absolutePath)
    testLogging {
        showStandardStreams = true
        events("passed", "failed", "skipped")
    }
}
