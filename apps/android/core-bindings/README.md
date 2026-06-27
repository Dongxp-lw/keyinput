# core-bindings（Android 绑定模块）

本目录是 Android 的 `core-bindings` Gradle 模块（[模块架构与层间契约](../../../docs/technical/module-architecture.md) §3）：
打包 Rust 共享核心的 `.so` 与 UniFFI 生成的 Kotlin 绑定，供 `app` / `keyboard` 依赖。
依赖方向：`app`/`keyboard` → `core-bindings` → `vault-core`（Rust）。

## 当前内容（L0-05c，已验证）

- `src/main/jniLibs/<abi>/libvault_core.so`：由 `cargo-ndk` 交叉编译的 4 个 ABI 库
  （arm64-v8a / armeabi-v7a / x86_64 / x86）。
- `src/main/kotlin/uniffi/vault_core/vault_core.kt`：由 `uniffi-bindgen` 生成的 Kotlin 绑定（含 `ping`）。

二者都是**生成产物**（已在根 `.gitignore` 忽略），构建时由下列命令重新生成。

## 重新生成（从仓库根目录运行）

前置：`ANDROID_NDK_HOME` 指向 NDK r27d；已安装 4 个 Rust Android target 与 `cargo-ndk`。

交叉编译各 ABI 的 `.so`：

    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 \
      -o apps/android/core-bindings/src/main/jniLibs build -p vault-core --release

生成 Kotlin 绑定：

    cargo run --features bindgen --bin uniffi-bindgen -- generate \
      --library apps/android/core-bindings/src/main/jniLibs/arm64-v8a/libvault_core.so \
      --language kotlin --out-dir apps/android/core-bindings/src/main/kotlin

## 待补（L0-05d）

- `build.gradle.kts`（android library；`jniLibs` srcDir；依赖 JNA；把上面两条命令接为 Gradle 任务）。
- `AndroidManifest.xml`，并与根 Gradle 工程（`settings.gradle.kts`、version catalog、wrapper）一并在 L0-05d 落地，
  `./gradlew assemble` 端到端验证（版本基线见 `.agent/decisions.md` D-013）。
