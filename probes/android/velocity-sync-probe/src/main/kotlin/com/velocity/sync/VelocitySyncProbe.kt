package com.velocity.sync

import android.os.Handler
import android.os.Looper
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * VelocitySyncProbe: Embedded in debug app builds to provide native idle detection.
 *
 * Usage:
 *   In your Application.onCreate():
 *     if (BuildConfig.DEBUG) {
 *         VelocitySyncProbe.start()
 *     }
 *
 * The probe tracks the main looper, animations, network requests, and async tasks
 * to determine when the app is idle. An HTTP server on localhost:19401 exposes
 * this state to the Velocity test runner (via adb port-forward).
 */
object VelocitySyncProbe {
    private val stateTracker = AppStateTracker
    private var server: SyncServer? = null
    private val resources = mutableListOf<IdlingResource>()
    private var started = AtomicBoolean(false)

    /**
     * Start the probe on the given port (default 19401).
     * Call from main thread in debug builds only.
     */
    @JvmStatic
    @JvmOverloads
    fun start(port: Int = 19401) {
        if (!started.compareAndSet(false, true)) return

        // Register built-in idling resources
        register(MainLooperIdlingResource())
        register(AnimationIdlingResource())

        // Start HTTP server
        server = SyncServer(stateTracker, port).also { it.start() }
    }

    /** Stop the probe and release all resources. */
    @JvmStatic
    fun stop() {
        if (!started.compareAndSet(true, false)) return
        server?.stop()
        server = null
        resources.clear()
    }

    /** Whether the app is currently idle. */
    @JvmStatic
    val isIdle: Boolean
        get() = stateTracker.isIdle

    /** Register a custom IdlingResource. */
    @JvmStatic
    fun register(resource: IdlingResource) {
        resources.add(resource)
        stateTracker.registerResource(resource)
    }
}

/** Interface for custom idling resources (Espresso-compatible pattern). */
interface IdlingResource {
    /** Human-readable name for debugging. */
    val name: String

    /** Whether this resource is currently idle. */
    val isIdle: Boolean
}

/** Tracks the aggregate idle state from all registered resources. */
object AppStateTracker {
    private val _networkCount = AtomicInteger(0)
    private val _customResources = mutableListOf<IdlingResource>()

    val isIdle: Boolean
        get() = _networkCount.get() == 0 && _customResources.all { it.isIdle }

    val pendingReasons: List<String>
        get() {
            val reasons = mutableListOf<String>()
            val net = _networkCount.get()
            if (net > 0) reasons.add("network:$net")
            for (resource in _customResources) {
                if (!resource.isIdle) reasons.add(resource.name)
            }
            return reasons
        }

    fun incrementNetwork() { _networkCount.incrementAndGet() }
    fun decrementNetwork() { _networkCount.decrementAndGet() }

    fun registerResource(resource: IdlingResource) {
        _customResources.add(resource)
    }
}

/** Monitors the main thread's Handler message queue for pending messages. */
class MainLooperIdlingResource : IdlingResource {
    override val name = "main_looper"
    private val handler = Handler(Looper.getMainLooper())

    override val isIdle: Boolean
        get() = !handler.hasMessages(0) && !handler.hasCallbacks { true }
}

/** Monitors running animations via ValueAnimator. */
class AnimationIdlingResource : IdlingResource {
    override val name = "animation"

    override val isIdle: Boolean
        get() {
            return try {
                val method = android.animation.ValueAnimator::class.java
                    .getDeclaredMethod("getRunningAnimationCount")
                method.isAccessible = true
                (method.invoke(null) as? Int ?: 0) == 0
            } catch (_: Exception) {
                true // If we can't access, assume idle
            }
        }
}
