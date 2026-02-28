plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.openclipboard"
    compileSdk = 35

    sourceSets {
        // Include generated UniFFI Kotlin bindings directly from the repo.
        getByName("main").java.srcDir("../../ffi/bindings/kotlin")
        getByName("test").java.srcDir("../../ffi/bindings/kotlin")
    }

    defaultConfig {
        applicationId = "com.openclipboard"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    kotlinOptions {
        jvmTarget = "11"
    }
    buildFeatures {
        compose = true
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

}

tasks.withType<Test> {
    // UniFFI Kotlin bindings use this property to load the native lib via absolute path.
    val lib = rootProject.projectDir.resolve("../target/debug/libopenclipboard_ffi.so")
    if (lib.exists()) {
        systemProperty("uniffi.component.openclipboard.libraryOverride", lib.absolutePath)
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.ui.tooling.preview)
    implementation(libs.androidx.material3)
    implementation(libs.google.material)
    implementation(libs.androidx.navigation.compose)

    // QR scan (CameraX + ML Kit)
    implementation(libs.androidx.camera.camera2)
    implementation(libs.androidx.camera.lifecycle)
    implementation(libs.androidx.camera.view)
    implementation(libs.google.mlkit.barcode.scanning)

    // QR show (ZXing)
    implementation(libs.zxing.core)

    // JNA: use @aar on Android so libjnidispatch.so is included for device ABIs.
    // Also include the JAR for JVM unit tests (host needs desktop native libs).
    implementation(libs.jna) { artifact { type = "aar" } }
    testImplementation("net.java.dev.jna:jna:5.16.0")
    
    debugImplementation(libs.androidx.ui.tooling)
    debugImplementation(libs.androidx.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation("org.robolectric:robolectric:4.12.2")
    testImplementation("androidx.test:core-ktx:1.6.1")
    testImplementation("androidx.test.ext:junit-ktx:1.2.1")

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.espresso.core)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules)
}