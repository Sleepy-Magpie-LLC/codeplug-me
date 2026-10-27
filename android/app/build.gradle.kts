import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// Codeplug app module. Builds the shared codeplugme-core Rust crate for Android
// ABIs via cargo-ndk before every build and packages the resulting cdylib
// (libcodeplugme_core.so) in jniLibs; the UniFFI-generated Kotlin bindings in
// src/main/java/uniffi/codeplugme_core load it through JNA.
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

// Release signing comes from android/key.properties (gitignored) when it exists:
// storeFile, storePassword, keyAlias, keyPassword. Without it the release APK is
// left unsigned.
val keystoreProperties = Properties().apply {
    val file = rootProject.file("key.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

android {
    namespace = "me.faulk.codeplugme"
    compileSdk = 35

    defaultConfig {
        applicationId = "me.faulk.codeplugme"
        // 29 is the floor for the connectedDevice foreground-service type that
        // keeps a multi-minute codeplug transfer alive when the app is
        // backgrounded.
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1"

        // Only the ABIs we actually cross-compile the core for. JNA's AAR
        // carries jnidispatch for several more (armeabi, mips, x86); without
        // this filter the APK would install on such a device, find JNA but no
        // libcodeplugme_core.so, and crash on first use rather than being refused
        // at install time. arm64-v8a covers every current phone; x86_64 is the
        // emulator.
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (keystoreProperties.isNotEmpty()) {
            create("release") {
                storeFile = rootProject.file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.all {
            // Lets host JVM unit tests load a desktop-built libcodeplugme_core
            // (produced by scripts/gen-bindings.sh) through JNA, so the codeplug
            // logic is testable with no device and no radio attached.
            it.systemProperty(
                "jna.library.path",
                rootProject.projectDir.parentFile.resolve("target/release").absolutePath
            )
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// Builds the codeplugme-core cdylib for both device (arm64-v8a) and emulator
// (x86_64) ABIs into src/main/jniLibs. Runs from the workspace root so cargo
// resolves the workspace.
//
// --no-default-features drops the `desktop` feature, and with it `serialport`:
// that crate reaches for libudev on Linux targets, which Android does not have.
// The transport comes from Kotlin instead (see usb/UsbRadioTransport.kt).
val cargoNdk = tasks.register<Exec>("cargoNdk") {
    group = "build"
    description = "Cross-compiles codeplugme-core for Android ABIs into jniLibs"
    workingDir = rootProject.projectDir.parentFile
    val home = System.getProperty("user.home")
    val androidHome = System.getenv("ANDROID_HOME") ?: "$home/Library/Android/sdk"
    environment("PATH", "$home/.cargo/bin:${System.getenv("PATH")}")
    environment("ANDROID_HOME", androidHome)
    commandLine(
        "$home/.cargo/bin/cargo", "ndk",
        "-t", "arm64-v8a", "-t", "x86_64",
        "-o", "android/app/src/main/jniLibs",
        "build", "--release", "-p", "codeplugme-core",
        "--no-default-features", "--features", "uniffi-api"
    )
}

tasks.named("preBuild") {
    dependsOn(cargoNdk)
}

// Builds the host-architecture cdylib the JVM unit tests load through JNA.
//
// This has to be a real task rather than a documented manual step: a plain
// `cargo build --release` produces a dylib with the DEFAULT features, which has
// no UniFFI symbols in it, and the tests then fail with an unresolved-symbol
// error that looks like a JNA problem rather than a stale-artifact one.
val hostCoreLib = tasks.register<Exec>("hostCoreLib") {
    group = "verification"
    description = "Builds codeplugme-core for the host with the UniFFI surface, for unit tests"
    workingDir = rootProject.projectDir.parentFile
    val home = System.getProperty("user.home")
    environment("PATH", "$home/.cargo/bin:${System.getenv("PATH")}")
    commandLine(
        "$home/.cargo/bin/cargo", "build", "--release",
        "-p", "codeplugme-core", "--features", "uniffi-api"
    )
}

tasks.withType<Test>().configureEach {
    dependsOn(hostCoreLib)
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.05.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.0")
    implementation("androidx.navigation:navigation-compose:2.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")

    // JNA backs the UniFFI bindings; @aar bundles the Android native dispatch libs.
    implementation("net.java.dev.jna:jna:5.17.0@aar")

    // USB host serial drivers: CDC/ACM (the AnyTone, detected by interface
    // class) and Prolific PL2303 (the KT-5000's programming cable).
    implementation("com.github.mik3y:usb-serial-for-android:3.9.0")

    // Host-JVM JNA so the unit tests can load the desktop dylib.
    testImplementation("net.java.dev.jna:jna:5.17.0")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
}
