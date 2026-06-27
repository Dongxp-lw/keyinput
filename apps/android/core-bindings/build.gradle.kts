// core-bindings：打包 Rust 核心的 .so（src/main/jniLibs）+ UniFFI 生成的 Kotlin 绑定
// （src/main/kotlin/uniffi）。供 app / keyboard 依赖。详见本模块 README。
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.lincdkeyinput.core"
    compileSdk = 35

    defaultConfig {
        minSdk = 28
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    // UniFFI 生成的 Kotlin 绑定运行期依赖 JNA。Android 必须用 @aar 制品
    // （内含各 ABI 的 libjnidispatch.so）；普通 jar 在设备上会 UnsatisfiedLinkError。
    api("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
}

// === Rust 核心交叉编译 + UniFFI 绑定生成：接为 Gradle 任务 ===
// 产物（jniLibs/*.so、src/main/kotlin/uniffi）已 gitignore，构建前由下列任务按需重新生成，
// 使全新检出也能可复现地构建。需环境：cargo、cargo-ndk、ANDROID_NDK_HOME，
// 以及 Windows gnu host 的 MinGW dlltool（均在用户 PATH/env，见 .agent/state.md 开发环境）。
val repoRoot: java.io.File = rootProject.projectDir.parentFile.parentFile
val coreCrate: java.io.File = repoRoot.resolve("core/vault-core")
val jniLibsDir = layout.projectDirectory.dir("src/main/jniLibs")
val uniffiKotlinDir = layout.projectDirectory.dir("src/main/kotlin")

val cargoNdkBuild = tasks.register<Exec>("cargoNdkBuild") {
    group = "rust"
    description = "用 cargo-ndk 交叉编译 vault-core 为各 ABI 的 .so"
    inputs.dir(coreCrate.resolve("src"))
    inputs.file(coreCrate.resolve("Cargo.toml"))
    outputs.dir(jniLibsDir)
    workingDir = repoRoot
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a", "-t", "armeabi-v7a", "-t", "x86_64", "-t", "x86",
        "-o", jniLibsDir.asFile.absolutePath,
        "build", "-p", "vault-core", "--release",
    )
}

val uniffiBindgen = tasks.register<Exec>("uniffiBindgen") {
    group = "rust"
    description = "用 uniffi-bindgen 由 .so 生成 Kotlin 绑定"
    dependsOn(cargoNdkBuild)
    inputs.dir(coreCrate.resolve("src"))
    outputs.dir(uniffiKotlinDir.dir("uniffi"))
    workingDir = repoRoot
    commandLine(
        "cargo", "run", "--features", "bindgen", "--bin", "uniffi-bindgen", "--",
        "generate",
        "--library", jniLibsDir.file("arm64-v8a/libvault_core.so").asFile.absolutePath,
        "--language", "kotlin",
        "--out-dir", uniffiKotlinDir.asFile.absolutePath,
    )
}

// 构建前先重新生成 .so 与绑定。
tasks.named("preBuild") {
    dependsOn(uniffiBindgen)
}
