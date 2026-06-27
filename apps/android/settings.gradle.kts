// 根 Gradle 设置：Android 多模块工程（[模块架构](../../docs/technical/module-architecture.md) §3）。
// Cargo 工作区在仓库根；本 Gradle 工程根在 apps/android。
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

rootProject.name = "private-input-vault-android"

include(":app", ":keyboard", ":core-bindings")
