plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.openclipboard"
    compileSdk = 34

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
    implementation(libs.androidx.navigation.compose)
    implementation(libs.jna)
    
    debugImplementation(libs.androidx.ui.tooling)
    debugImplementation(libs.androidx.ui.test.manifest)

    testImplementation(libs.junit)
}