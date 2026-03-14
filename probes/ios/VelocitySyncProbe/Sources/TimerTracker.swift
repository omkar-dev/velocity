import Foundation

/// Tracks active non-repeating timers to detect pending timer-based work.
final class TimerTracker {
    private let stateTracker: AppStateTracker

    init(stateTracker: AppStateTracker) {
        self.stateTracker = stateTracker
    }

    func install() {
        // Swizzle Timer.scheduledTimer to intercept timer creation
        // We track non-repeating timers as they indicate pending work
        Self.swizzle(
            cls: Timer.self,
            original: #selector(Timer.scheduledTimer(withTimeInterval:repeats:block:) as (TimeInterval, Bool, @escaping (Timer) -> Void) -> Timer),
            replacement: #selector(Timer.velocity_scheduledTimer(withTimeInterval:repeats:block:))
        )
    }

    private static func swizzle(cls: AnyClass, original: Selector, replacement: Selector) {
        guard let originalMethod = class_getClassMethod(cls, original),
              let replacementMethod = class_getClassMethod(cls, replacement)
        else { return }
        method_exchangeImplementations(originalMethod, replacementMethod)
    }
}

extension Timer {
    @objc class func velocity_scheduledTimer(
        withTimeInterval interval: TimeInterval,
        repeats: Bool,
        block: @escaping (Timer) -> Void
    ) -> Timer {
        if !repeats {
            AppStateTracker.shared.incrementTimer()
            let wrappedBlock: (Timer) -> Void = { timer in
                block(timer)
                AppStateTracker.shared.decrementTimer()
            }
            return velocity_scheduledTimer(withTimeInterval: interval, repeats: repeats, block: wrappedBlock)
        }
        return velocity_scheduledTimer(withTimeInterval: interval, repeats: repeats, block: block)
    }
}
