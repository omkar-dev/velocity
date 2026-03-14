import Foundation

/// Central registry tracking all sources of app busyness.
/// Each tracker reports its state via atomic operations.
public final class AppStateTracker {
    public static let shared = AppStateTracker()

    private let _runLoopBusy = AtomicBool()
    private let _animationCount = AtomicCounter()
    private let _networkCount = AtomicCounter()
    private let _timerCount = AtomicCounter()
    private let _rnBridgeBusy = AtomicBool()
    private var _rnTrackingEnabled = false

    private init() {}

    /// Whether the app is idle (no pending work from any tracker).
    public var isIdle: Bool {
        !_runLoopBusy.value
            && _animationCount.value == 0
            && _networkCount.value == 0
            && _timerCount.value == 0
            && (!_rnTrackingEnabled || !_rnBridgeBusy.value)
    }

    /// List of reasons the app is not idle.
    public var pendingReasons: [String] {
        var reasons: [String] = []
        if _runLoopBusy.value { reasons.append("runloop") }
        let anim = _animationCount.value
        if anim > 0 { reasons.append("animation:\(anim)") }
        let net = _networkCount.value
        if net > 0 { reasons.append("network:\(net)") }
        let timer = _timerCount.value
        if timer > 0 { reasons.append("timer:\(timer)") }
        if _rnTrackingEnabled && _rnBridgeBusy.value { reasons.append("rn_bridge") }
        return reasons
    }

    // MARK: - Registration methods

    public func markRunLoopBusy(_ busy: Bool) { _runLoopBusy.set(busy) }
    public func incrementNetwork() { _networkCount.increment() }
    public func decrementNetwork() { _networkCount.decrement() }
    public func incrementAnimation() { _animationCount.increment() }
    public func decrementAnimation() { _animationCount.decrement() }
    public func incrementTimer() { _timerCount.increment() }
    public func decrementTimer() { _timerCount.decrement() }
    public func markRNBridgeBusy(_ busy: Bool) { _rnBridgeBusy.set(busy) }

    func enableRNTracking() { _rnTrackingEnabled = true }
}

// MARK: - Thread-safe atomic primitives

final class AtomicBool {
    private var _value: Int32 = 0
    var value: Bool { OSAtomicAdd32(0, &_value) != 0 }
    func set(_ newValue: Bool) {
        if newValue {
            OSAtomicTestAndSet(0, &_value)
        } else {
            OSAtomicTestAndClear(0, &_value)
        }
    }
}

final class AtomicCounter {
    private var _value: Int32 = 0
    var value: Int { Int(OSAtomicAdd32(0, &_value)) }
    func increment() { OSAtomicIncrement32(&_value) }
    func decrement() { OSAtomicDecrement32(&_value) }
}
