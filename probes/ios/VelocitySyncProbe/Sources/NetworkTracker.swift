import Foundation

/// Tracks in-flight URLSession data tasks to detect pending network requests.
final class NetworkTracker {
    private let stateTracker: AppStateTracker

    init(stateTracker: AppStateTracker) {
        self.stateTracker = stateTracker
    }

    func install() {
        // Swizzle URLSessionTask.resume to intercept network request starts
        Self.swizzle(
            cls: URLSessionTask.self,
            original: #selector(URLSessionTask.resume),
            replacement: #selector(URLSessionTask.velocity_resume)
        )
    }

    static func taskDidStart() {
        AppStateTracker.shared.incrementNetwork()
    }

    static func taskDidFinish() {
        AppStateTracker.shared.decrementNetwork()
    }

    private static func swizzle(cls: AnyClass, original: Selector, replacement: Selector) {
        guard let originalMethod = class_getInstanceMethod(cls, original),
              let replacementMethod = class_getInstanceMethod(cls, replacement)
        else { return }
        method_exchangeImplementations(originalMethod, replacementMethod)
    }
}

extension URLSessionTask {
    @objc func velocity_resume() {
        NetworkTracker.taskDidStart()

        // Observe task completion via KVO on state
        let observer = TaskStateObserver()
        objc_setAssociatedObject(self, &TaskStateObserver.key, observer, .OBJC_ASSOCIATION_RETAIN)
        addObserver(observer, forKeyPath: "state", options: [.new], context: nil)

        velocity_resume() // calls original (swizzled)
    }
}

private class TaskStateObserver: NSObject {
    static var key: UInt8 = 0
    private var didFinish = false

    override func observeValue(
        forKeyPath keyPath: String?,
        of object: Any?,
        change: [NSKeyValueChangeKey: Any]?,
        context: UnsafeMutableRawPointer?
    ) {
        guard keyPath == "state",
              let task = object as? URLSessionTask,
              !didFinish
        else { return }

        if task.state == .completed || task.state == .canceling {
            didFinish = true
            NetworkTracker.taskDidFinish()
            task.removeObserver(self, forKeyPath: "state")
        }
    }
}
