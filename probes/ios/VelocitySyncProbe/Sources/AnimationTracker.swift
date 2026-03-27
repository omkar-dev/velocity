import QuartzCore
import UIKit

/// Tracks active CALayer animations to detect when the UI is animating.
///
/// **Carousel tolerance**: Repeating animations (e.g., auto-scrolling carousels,
/// looping progress indicators) are excluded from the idle count after a grace
/// period. Only finite, transient animations block idle detection.
final class AnimationTracker {
    static var shared: AnimationTracker?

    private let stateTracker: AppStateTracker
    private var displayLink: CADisplayLink?
    private var trackedLayers: NSHashTable<CALayer> = .weakObjects()
    private var lastAnimationCount: Int = 0

    /// Layers whose animations have been continuously active for longer than
    /// `continuousAnimationGracePeriod` are considered "ambient" and excluded
    /// from idle blocking.
    private var layerFirstSeen: [ObjectIdentifier: CFTimeInterval] = [:]

    /// How long an animation must run continuously before it's classified as
    /// ambient/repeating and excluded from the idle count (default: 2 seconds).
    var continuousAnimationGracePeriod: CFTimeInterval = 2.0

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
        let now = CACurrentMediaTime()
        var activeCount = 0
        var currentLayerIds = Set<ObjectIdentifier>()

        for layer in trackedLayers.allObjects {
            guard let keys = layer.animationKeys(), !keys.isEmpty else { continue }

            let layerId = ObjectIdentifier(layer)
            currentLayerIds.insert(layerId)

            // Track when we first saw this layer animating
            if layerFirstSeen[layerId] == nil {
                layerFirstSeen[layerId] = now
            }

            let firstSeen = layerFirstSeen[layerId] ?? now
            let duration = now - firstSeen

            // Check if ALL animations on this layer are repeating/infinite
            let allRepeating = keys.allSatisfy { key in
                guard let anim = layer.animation(forKey: key) else { return false }
                return anim.repeatCount == .infinity
                    || anim.repeatCount > 100
                    || (anim.repeatCount > 0 && anim.repeatDuration > self.continuousAnimationGracePeriod)
            }

            // Exclude layer if it's been continuously animating past the grace period
            // AND all its animations are repeating (carousel, spinner, etc.)
            if allRepeating && duration > continuousAnimationGracePeriod {
                continue // ambient animation — don't count toward idle blocking
            }

            activeCount += 1
        }

        // Clean up tracking for layers that stopped animating
        layerFirstSeen = layerFirstSeen.filter { currentLayerIds.contains($0.key) }

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
        let layerId = ObjectIdentifier(layer)
        layerFirstSeen.removeValue(forKey: layerId)
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
