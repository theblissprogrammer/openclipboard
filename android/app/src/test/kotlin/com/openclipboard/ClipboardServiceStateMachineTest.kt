package com.openclipboard

import com.openclipboard.service.ClipboardService
import com.openclipboard.service.ClipboardServiceStateMachine
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ClipboardServiceStateMachineTest {

    @Test
    fun startIsIdempotentAndStopTransitionsCorrectly() {
        val sm = ClipboardServiceStateMachine()

        // null action is treated like START (system restart)
        assertTrue(sm.onStartCommand(null) is ClipboardServiceStateMachine.Effect.Start)
        assertEquals(ClipboardServiceStateMachine.State.Running, sm.state())

        // Starting again should noop.
        assertTrue(sm.onStartCommand(ClipboardService.ACTION_START) is ClipboardServiceStateMachine.Effect.Noop)
        assertEquals(ClipboardServiceStateMachine.State.Running, sm.state())

        // Stop.
        assertTrue(sm.onStartCommand(ClipboardService.ACTION_STOP) is ClipboardServiceStateMachine.Effect.Stop)
        assertEquals(ClipboardServiceStateMachine.State.Stopped, sm.state())

        // Stop again should noop.
        assertTrue(sm.onStartCommand(ClipboardService.ACTION_STOP) is ClipboardServiceStateMachine.Effect.Noop)
        assertEquals(ClipboardServiceStateMachine.State.Stopped, sm.state())
    }
}
