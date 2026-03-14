import Foundation

/// VelocitySyncProbe: Embedded in debug app builds to provide native idle detection.
///
/// Usage:
///   In your AppDelegate or @main:
///     #if DEBUG
///     VelocitySyncProbe.start()
///     #endif
///
/// The probe tracks RunLoop, animations, network requests, and timers to determine
/// when the app is idle. A TCP server on localhost:19400 exposes this state to
/// the Velocity test runner.
public final class VelocitySyncProbe {
    public static let shared = VelocitySyncProbe()

    private let stateTracker = AppStateTracker.shared
    private var server: SyncServer?
    private var runLoopObserver: RunLoopObserver?
    private var animationTracker: AnimationTracker?
    private var networkTracker: NetworkTracker?
    private var timerTracker: TimerTracker?

    private init() {}

    /// Start the probe on the given port (default 19400).
    public static func start(port: UInt16 = 19400) {
        shared.startInternal(port: port)
    }

    /// Stop the probe and close the server.
    public static func stop() {
        shared.stopInternal()
    }

    /// Whether the app is currently idle (all trackers report no pending work).
    public static var isIdle: Bool {
        shared.stateTracker.isIdle
    }

    /// Enable React Native bridge tracking (optional).
    public static func enableRN() {
        shared.stateTracker.enableRNTracking()
    }

    private func startInternal(port: UInt16) {
        // Install trackers
        runLoopObserver = RunLoopObserver(stateTracker: stateTracker)
        runLoopObserver?.install()

        animationTracker = AnimationTracker(stateTracker: stateTracker)
        animationTracker?.install()

        networkTracker = NetworkTracker(stateTracker: stateTracker)
        networkTracker?.install()

        timerTracker = TimerTracker(stateTracker: stateTracker)
        timerTracker?.install()

        // Start TCP server
        server = SyncServer(stateTracker: stateTracker, port: port)
        server?.start()
    }

    private func stopInternal() {
        server?.stop()
        server = nil
        runLoopObserver?.uninstall()
        runLoopObserver = nil
        animationTracker = nil
        networkTracker = nil
        timerTracker = nil
    }
}
