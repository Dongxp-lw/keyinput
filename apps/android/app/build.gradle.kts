// app：主应用模块（L4 实现）。当前为骨架，仅演示经 core-bindings 调用 Rust 核心。
// 离线优先（[工程基础](../../../docs/technical/engineering-foundation.md) §5.4）：不声明 INTERNET 权限。
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.lincdkeyinput.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.lincdkeyinput"
        minSdk = 28
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
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
    implementation(project(":core-bindings"))

    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
}
