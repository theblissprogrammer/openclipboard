package com.openclipboard

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.ActivityTestRule
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Minimal emulator smoke test: if the app crashes on launch, this will fail.
 */
@RunWith(AndroidJUnit4::class)
class LaunchSmokeTest {

    @get:Rule
    val rule = ActivityTestRule(MainActivity::class.java, /* initialTouchMode */ true, /* launchActivity */ true)

    @Test
    fun app_launches() {
        // If MainActivity crashes during startup, the test run will fail before reaching here.
        // We keep this intentionally minimal and fast.
        rule.activity
    }
}
