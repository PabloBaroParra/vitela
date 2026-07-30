import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.vitela.pdf"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.vitela.pdf"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDir("src/main/jniLibs")
            java.srcDir("build/generated/uniffi/kotlin")
            // The generated META-INF/services descriptor that lets
            // PdfCoreProvider find GeneratedPdfCoreFactory. It must be a
            // *resources* dir: a java.srcDir is compiled but never packaged,
            // so the descriptor would not reach the APK and the app would
            // report native support as missing even with the .so files present.
            resources.srcDir("build/generated/uniffi/resources")
            // The sample document is packaged straight from the shared
            // assets/ directory instead of being copied into the module, so
            // all three shells ship the byte-identical file produced by
            // `cargo run -p gen-sample`.
            assets.srcDir("../../../assets/sample")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.activity:activity-compose:1.10.0")
    implementation("androidx.compose.material3:material3")
    // Used directly by the continuous reader (LazyColumn), not just pulled in
    // behind material3 — declared so the dependency survives a material3 bump.
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-ktx:2.8.7")

    // UniFFI's generated Kotlin bindings call into the native library through
    // JNA (`com.sun.jna.*`). The `@aar` classifier is required: the plain jar
    // does not ship the Android native dispatch libraries.
    implementation("net.java.dev.jna:jna:5.17.0@aar")

    debugImplementation("androidx.compose.ui:ui-tooling")
    testImplementation("junit:junit:4.13.2")
}
