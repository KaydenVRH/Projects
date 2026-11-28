import AppKit
import QuartzCore

/// Something that happens now and then — a tube flickering, a glint running the
/// bar — played as a one-shot on a timer rather than as a forever-repeating
/// animation.
///
/// The difference is what the window server does in between. While any
/// animation is attached to a layer, it recomposites that display every frame
/// for the animation's whole cycle, even the long stretches where nothing on it
/// moves; the always-on decorations did that on every display, all the time,
/// and roughly doubled the window server's load. A one-shot is gone when it
/// ends, so between flickers the screen is left alone.
final class Recurring {
    private var timer: Timer?

    /// Calls `fire` after `delay`, then every `period` seconds until cancelled
    /// or released.
    init(every period: TimeInterval, after delay: TimeInterval = 0, _ fire: @escaping () -> Void) {
        let timer = Timer(fire: Date().addingTimeInterval(delay), interval: period, repeats: true) { _ in fire() }
        timer.tolerance = period * 0.05
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    /// A looping keyframe track that holds its rest value for most of each
    /// cycle — values like `[1, 1, 0.2, 1, 1]`, where the first and last
    /// keyframes only hold the rest value — replayed as just its burst, once a
    /// cycle, at the same point in the cycle.
    convenience init(replaying loop: CAKeyframeAnimation, on layer: CALayer, forKey key: String) {
        let times = (loop.keyTimes ?? []).map(\.doubleValue)
        let values = loop.values ?? []
        let cycle = loop.duration
        precondition(times.count == values.count && times.count > 3, "a burst needs held ends")
        let start = times[1], end = times[times.count - 2]
        let burst = CAKeyframeAnimation(keyPath: loop.keyPath)
        burst.values = Array(values[1..<(values.count - 1)])
        burst.keyTimes = times[1..<(times.count - 1)].map { NSNumber(value: ($0 - start) / (end - start)) }
        burst.duration = cycle * (end - start)
        burst.timingFunction = loop.timingFunction
        self.init(every: cycle, after: cycle * start) { [weak layer] in
            layer?.add(burst, forKey: key)
        }
    }

    /// Seconds until the wall clock next reaches a multiple of `period`, so
    /// separate surfaces on that period fire together and rest together:
    /// compositing stops only when nothing on any display is moving.
    static func untilNext(_ period: TimeInterval) -> TimeInterval {
        period - Date().timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period)
    }

    func cancel() {
        timer?.invalidate()
        timer = nil
    }

    deinit { timer?.invalidate() }
}

extension CAEmitterLayer {
    /// Emits in gusts: on for `gust` seconds every `period`, off in between.
    /// A layer with live particles is redrawn every frame, so a steady trickle
    /// keeps its display compositing forever; gusts let it rest once the last
    /// particle of each has faded.
    /// Gusts start on the wall clock's multiples of `period`, in step with
    /// every other surface gusting on that period.
    func gusting(every period: TimeInterval, for gust: TimeInterval) -> Recurring {
        birthRate = 0
        return Recurring(every: period, after: Recurring.untilNext(period)) { [weak self] in
            guard let self else { return }
            self.beginTime = CACurrentMediaTime()
            self.birthRate = 1
            DispatchQueue.main.asyncAfter(deadline: .now() + gust) { [weak self] in self?.birthRate = 0 }
        }
    }
}
