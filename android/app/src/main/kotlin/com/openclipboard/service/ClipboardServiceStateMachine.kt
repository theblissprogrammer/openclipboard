package com.openclipboard.service

/**
 * Pure Kotlin state machine so we can unit test idempotent service start/stop handling.
 */
class ClipboardServiceStateMachine {

    enum class State {
        Stopped,
        Running,
    }

    sealed class Effect {
        data object Start : Effect()
        data object Stop : Effect()
        data object Noop : Effect()
    }

    private var state: State = State.Stopped

    fun state(): State = state

    /**
     * Maps service Intent actions to side-effect instructions.
     *
     * - ACTION_START: Start if stopped, otherwise noop.
     * - ACTION_STOP: Stop if running, otherwise noop.
     * - null/unknown: Treat as ACTION_START (START_STICKY restart case).
     */
    fun onStartCommand(action: String?): Effect {
        val a = action ?: ClipboardService.ACTION_START
        return when (a) {
            ClipboardService.ACTION_START -> {
                if (state == State.Running) Effect.Noop else {
                    state = State.Running
                    Effect.Start
                }
            }

            ClipboardService.ACTION_STOP -> {
                if (state == State.Stopped) Effect.Noop else {
                    state = State.Stopped
                    Effect.Stop
                }
            }

            else -> {
                // Default to start.
                if (state == State.Running) Effect.Noop else {
                    state = State.Running
                    Effect.Start
                }
            }
        }
    }
}
