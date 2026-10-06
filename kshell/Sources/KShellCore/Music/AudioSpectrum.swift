import AVFoundation
import Accelerate
import Foundation

/// Turns the player's audio into the cava-style bands the visualiser draws.
///
/// The mixer hands us ~512 frames at a time. Those slide into a 4096-point
/// window (an FFT every callback, over the last eight buffers), which is what
/// spreads the low end across several bins — at 1024 the bottom dozen bands all
/// read the same bin and moved as one slab, and at 2048 they still bunched into
/// about one. Each band then takes several interpolated samples across its
/// range, so neighbours stay distinct even where a band is narrower than a bin.
///
/// Responsiveness comes from the small hop and the smoothing, not a short
/// window: the band value is the peak across the window, so a transient shows up
/// within one hop of it starting, and at 512 frames that is ~11 ms.
///
/// Levels are smoothed with a fast attack and a slow release so bars leap and
/// settle instead of flickering. Nothing here touches the output: the tap is a
/// copy of the mix, so it costs a little CPU and no permissions.
public final class AudioSpectrum: ObservableObject {
    /// 0…1 per band, lowest frequency first.
    @Published public private(set) var bands: [Float] = []

    /// How many bands to fold the spectrum into. The visualiser's config sets it.
    public var bandCount: Int = 64 {
        didSet {
            guard bandCount != oldValue, bandCount > 0 else { return }
            smoothed = Array(repeating: 0, count: bandCount)
            bands = smoothed
            rebuildBands()
        }
    }

    /// Set while something is playing: an idle shell does no FFT work at all.
    public var isEnabled = false

    private let fftSize = 4096
    private var setup: FFTSetup?
    private var log2n: vDSP_Length = 12
    private var window: [Float] = []
    /// The most recent `fftSize` mono samples, oldest first.
    private var history: [Float] = []
    /// Scratch for one callback's mono fold.
    private var incoming: [Float] = []
    private var windowed: [Float] = []
    private var real: [Float] = []
    private var imag: [Float] = []
    private var magnitudes: [Float] = []
    private var smoothed: [Float] = []
    /// Bin-space positions to sample per band (lowest to highest within it).
    private var bandSamples: [[Float]] = []
    private var sampleRate: Double = 44100

    /// -60 dB … 0 dB mapped onto the bars.
    private let floorDB: Float = 60
    /// Response curve applied on top. 1 is linear; lower lifts quiet detail.
    private let response: Float = 0.62
    /// How many points across each band to sample.
    private let samplesPerBand = 5

    public init() {
        log2n = vDSP_Length(log2(Double(fftSize)).rounded())
        setup = vDSP_create_fftsetup(log2n, FFTRadix(kFFTRadix2))
        window = [Float](repeating: 0, count: fftSize)
        vDSP_hann_window(&window, vDSP_Length(fftSize), Int32(vDSP_HANN_NORM))
        history = []
        history.reserveCapacity(fftSize)
        incoming = [Float](repeating: 0, count: 16384)
        windowed = [Float](repeating: 0, count: fftSize)
        real = [Float](repeating: 0, count: fftSize / 2)
        imag = [Float](repeating: 0, count: fftSize / 2)
        magnitudes = [Float](repeating: 0, count: fftSize / 2)
        smoothed = Array(repeating: 0, count: bandCount)
        bands = smoothed
        rebuildBands()
    }

    /// Fold the current sample rate into band → bin-sample positions.
    private func rebuildBands() {
        let binWidth = sampleRate / Double(fftSize)
        let low = 30.0
        let high = min(16000.0, sampleRate / 2)
        guard binWidth > 0, high > low, bandCount > 0 else { return }

        bandSamples = (0..<bandCount).map { band in
            let f0 = low * pow(high / low, Double(band) / Double(bandCount))
            let f1 = low * pow(high / low, Double(band + 1) / Double(bandCount))
            return (0..<samplesPerBand).map { step in
                let ratio = Double(step) / Double(samplesPerBand - 1)
                return Float(f0 * pow(f1 / f0, ratio) / binWidth)
            }
        }
    }

    /// Called on the audio thread with the mixer's latest output.
    public func process(_ buffer: AVAudioPCMBuffer) {
        guard isEnabled, let setup, let channels = buffer.floatChannelData else { return }
        let frames = Int(buffer.frameLength)
        guard frames > 0, frames <= incoming.count else { return }

        if buffer.format.sampleRate != sampleRate, buffer.format.sampleRate > 0 {
            sampleRate = buffer.format.sampleRate
            rebuildBands()
        }

        // Average the channels into `incoming`, then slide the window along.
        let channelCount = Int(buffer.format.channelCount)
        if channelCount > 1 {
            for i in 0..<frames {
                var sum: Float = 0
                for c in 0..<channelCount { sum += channels[c][i] }
                incoming[i] = sum / Float(channelCount)
            }
        } else {
            for i in 0..<frames { incoming[i] = channels[0][i] }
        }

        history.append(contentsOf: incoming[0..<frames])
        if history.count > fftSize { history.removeFirst(history.count - fftSize) }
        // Wait for a full window the first time round; after that it slides.
        guard history.count == fftSize else { return }

        vDSP_vmul(history, 1, window, 1, &windowed, 1, vDSP_Length(fftSize))

        real.withUnsafeMutableBufferPointer { realBuffer in
            imag.withUnsafeMutableBufferPointer { imagBuffer in
                magnitudes.withUnsafeMutableBufferPointer { magnitudeBuffer in
                    windowed.withUnsafeBufferPointer { windowedBuffer in
                        guard let realBase = realBuffer.baseAddress,
                              let imagBase = imagBuffer.baseAddress,
                              let magnitudeBase = magnitudeBuffer.baseAddress,
                              let windowedBase = windowedBuffer.baseAddress else { return }
                        var split = DSPSplitComplex(realp: realBase, imagp: imagBase)
                        windowedBase.withMemoryRebound(to: DSPComplex.self, capacity: fftSize / 2) { packed in
                            vDSP_ctoz(packed, 2, &split, 1, vDSP_Length(fftSize / 2))
                            vDSP_fft_zrip(setup, &split, 1, log2n, FFTDirection(FFT_FORWARD))
                            vDSP_zvabs(&split, 1, magnitudeBase, 1, vDSP_Length(fftSize / 2))
                        }
                    }
                }
            }
        }

        // zrip's forward transform is scaled by 2; normalise to a sine's amplitude.
        let scale = 1 / Float(fftSize)
        let lastBin = fftSize / 2 - 2
        for (band, positions) in bandSamples.enumerated() {
            var peak: Float = 0
            for position in positions {
                let clamped = min(max(position, 0), Float(lastBin))
                let bin = Int(clamped)
                let fraction = clamped - Float(bin)
                let value = magnitudes[bin] + (magnitudes[bin + 1] - magnitudes[bin]) * fraction
                if value > peak { peak = value }
            }
            let db = min(max((20 * log10(peak * scale + 1e-7) + floorDB) / floorDB, 0), 1)
            // A gentle curve, well short of a square root: quiet detail reaches
            // further up the bar while silence still sits at zero.
            let level = pow(db, response)
            let previous = smoothed[band]
            // Fast attack, slow release: the classic cava fall.
            smoothed[band] = level > previous
                ? previous + (level - previous) * 0.72
                : previous + (level - previous) * 0.19
        }

        let snapshot = smoothed
        DispatchQueue.main.async { [weak self] in
            self?.bands = snapshot
        }
    }
}
