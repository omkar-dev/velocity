import Foundation
import CoreFoundation

/// Monitors the main RunLoop to detect when the UI thread is idle.
final class RunLoopObserver {
    private var observer: CFRunLoopObserver?
    private let stateTracker: AppStateTracker

    init(stateTracker: AppStateTracker) {
        self.stateTracker = stateTracker
    }

    func install() {
        let activities: CFRunLoopActivity = [
            .beforeWaiting,
            .afterWaiting,
            .beforeTimers,
            .beforeSources
        ]

        observer = CFRunLoopObserverCreateWithHandler(
            kCFAllocatorDefault,
            activities.rawValue,
            true,
            0,
            { [weak self] _, activity in
                guard let self else { return }
                switch activity {
                case .beforeWaiting:
                    self.stateTracker.markRunLoopBusy(false)
                case .afterWaiting, .beforeTimers, .beforeSources:
                    self.stateTracker.markRunLoopBusy(true)
                default:
                    break
                }
            }
        )

        if let observer {
            CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        }
    }

    func uninstall() {
        if let observer {
            CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .commonModes)
        }
        observer = nil
    }
}
