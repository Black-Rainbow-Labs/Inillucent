// The Android library for inillucent. `inillucent` is the only module: the AAR an application
// depends on, and its instrumented tests, which run the driver's conformance suite on a device.

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
    }
}

rootProject.name = "inillucent-android"
include(":inillucent")
