// vault-data：共享数据层（L4-FFI）。封装 UniFFI 生成的 VaultCore（经 :core-bindings）、
// 本地文件持久化、生物识别 Keystore 封装、剪贴板兜底。供 :app 与 :keyboard 复用（各自实例，
// 不共享会话；键盘为跨进程独立会话）。
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.lincdkeyinput.data"
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
    // api：把 UniFFI 生成的 VaultCore 类型与 androidx.biometric 透传给依赖方（:app / :keyboard）。
    api(project(":core-bindings"))
    api(libs.androidx.biometric)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.kotlinx.coroutines.android)
}
