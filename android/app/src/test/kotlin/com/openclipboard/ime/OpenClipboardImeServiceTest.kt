package com.openclipboard.ime

import android.view.View
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class OpenClipboardImeServiceTest {

    @Test
    fun `service creates input view`() {
        val controller = Robolectric.buildService(OpenClipboardImeService::class.java)
        val service = controller.create().get()

        val view: View? = service.onCreateInputView()
        assertNotNull(view)
    }
}
