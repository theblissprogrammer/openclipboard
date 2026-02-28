package com.openclipboard.service

import org.junit.Assert.assertEquals
import org.junit.Test

class ClipboardServiceStateMachineTest {

    @Test
    fun start_isIdempotent() {
        val sm = ClipboardServiceStateMachine()

        assertEquals(ClipboardServiceStateMachine.State.Stopped, sm.state())
        assertEquals(ClipboardServiceStateMachine.Effect.Start, sm.onStartCommand(ClipboardService.ACTION_START))
        assertEquals(ClipboardServiceStateMachine.State.Running, sm.state())

        assertEquals(ClipboardServiceStateMachine.Effect.Noop, sm.onStartCommand(ClipboardService.ACTION_START))
        assertEquals(ClipboardServiceStateMachine.State.Running, sm.state())
    }

    @Test
    fun stop_isIdempotent() {
        val sm = ClipboardServiceStateMachine()

        assertEquals(ClipboardServiceStateMachine.Effect.Noop, sm.onStartCommand(ClipboardService.ACTION_STOP))
        assertEquals(ClipboardServiceStateMachine.State.Stopped, sm.state())

        sm.onStartCommand(ClipboardService.ACTION_START)
        assertEquals(ClipboardServiceStateMachine.State.Running, sm.state())

        assertEquals(ClipboardServiceStateMachine.Effect.Stop, sm.onStartCommand(ClipboardService.ACTION_STOP))
        assertEquals(ClipboardServiceStateMachine.State.Stopped, sm.state())

        assertEquals(ClipboardServiceStateMachine.Effect.Noop, sm.onStartCommand(ClipboardService.ACTION_STOP))
    }

    @Test
    fun nullAction_defaultsToStart() {
        val sm = ClipboardServiceStateMachine()
        assertEquals(ClipboardServiceStateMachine.Effect.Start, sm.onStartCommand(null))
        assertEquals(ClipboardServiceStateMachine.State.Running, sm.state())
    }
}
