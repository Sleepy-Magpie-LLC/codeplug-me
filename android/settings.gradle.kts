// Gradle settings for the Codeplug Android app. The app module wraps the shared
// codeplugme-core Rust crate (built via cargo-ndk, see app/build.gradle.kts).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // usb-serial-for-android is published through JitPack.
        maven { url = uri("https://jitpack.io") }
    }
}

rootProject.name = "CodeplugMe"
include(":app")
