import QuartzCore
import UIKit

/// Tracks active CALayer animations to detect when the UI is animating.
final class AnimationTracker {
    static var shared: AnimationTracker?

    private let stateTracker: AppStateTracker
    private var displayLink: CADisplayLink?
    private var trackedLayers: NSHashTable<CALayer> = .weakObjects()
    private var lastAnimationCount: Int = 0

    init(stateTracker: AppStateTracker) {
        self.stateTracker = stateTracker
    }

    func install() {
        AnimationTracker.shared = self

        // Swizzle CALayer animation methods
        Self.swizzle(
            cls: CALayer.self,
            original: #selector(CALayer.add(_:forKey:)),
            replacement: #selector(CALayer.velocity_addAnimation(_:forKey:))
        )
        Self.swizzle(
            cls: CALayer.self,
            original: #selector(CALayer.removeAllAnimations),
            replacement: #selector(CALayer.velocity_removeAllAnimations)
        )

        // Display link for periodic reconciliation
        displayLink = CADisplayLink(target: self, selector: #selector(tick))
        displayLink?.add(to: .main, forMode: .common)
    }

    @objc private func tick() {
        var activeCount = 0
        for layer in trackedLayers.allObjects {
            if let keys = layer.animationKeys(), !keys.isEmpty {
                activeCount += 1
            }
        }

        let delta = activeCount - lastAnimationCount
        if delta > 0 {
            for _ in 0..<delta { stateTracker.incrementAnimation() }
        } else if delta < 0 {
            for _ in 0..<(-delta) { stateTracker.decrementAnimation() }
        }
        lastAnimationCount = activeCount
    }

    func trackLayer(_ layer: CALayer) {
        trackedLayers.add(layer)
    }

    func untrackLayer(_ layer: CALayer) {
        trackedLayers.remove(layer)
    }

    private static func swizzle(cls: AnyClass, original: Selector, replacement: Selector) {
        guard let originalMethod = class_getInstanceMethod(cls, original),
              let replacementMethod = class_getInstanceMethod(cls, replacement)
        else { return }
        method_exchangeImplementations(originalMethod, replacementMethod)
    }
}

extension CALayer {
    @objc func velocity_addAnimation(_ animation: CAAnimation, forKey key: String?) {
        AnimationTracker.shared?.trackLayer(self)
        velocity_addAnimation(animation, forKey: key) // calls original (swizzled)
    }

    @objc func velocity_removeAllAnimations() {
        velocity_removeAllAnimations() // calls original
        AnimationTracker.shared?.untrackLayer(self)
    }
}
