import CoreAudio
import Foundation

/// Default-output volume + mute via CoreAudio.
public enum SystemAudio {
    public struct State {
        public var volume: Float
        public var muted: Bool
    }

    public static func defaultOutput() -> State {
        guard let device = defaultOutputDevice() else {
            return State(volume: 0, muted: false)
        }
        let volume = readFloat(
            device: device,
            selector: kAudioDevicePropertyVolumeScalar,
            scope: kAudioDevicePropertyScopeOutput
        ) ?? 0
        let muted = (readUInt32(
            device: device,
            selector: kAudioDevicePropertyMute,
            scope: kAudioDevicePropertyScopeOutput
        ) ?? 0) != 0
        return State(volume: volume, muted: muted)
    }

    private static func defaultOutputDevice() -> AudioDeviceID? {
        var device = AudioDeviceID(0)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address, 0, nil, &size, &device
        )
        return status == noErr ? device : nil
    }

    /// Tries the main element, then the first two channels (many devices only
    /// expose per-channel volume).
    private static func readFloat(
        device: AudioDeviceID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope
    ) -> Float? {
        for element in [kAudioObjectPropertyElementMain, AudioObjectPropertyElement(1), AudioObjectPropertyElement(2)] {
            var value = Float(0)
            var size = UInt32(MemoryLayout<Float>.size)
            var address = AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: element)
            if AudioObjectGetPropertyData(device, &address, 0, nil, &size, &value) == noErr {
                return value
            }
        }
        return nil
    }

    private static func readUInt32(
        device: AudioDeviceID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope
    ) -> UInt32? {
        for element in [kAudioObjectPropertyElementMain, AudioObjectPropertyElement(1), AudioObjectPropertyElement(2)] {
            var value = UInt32(0)
            var size = UInt32(MemoryLayout<UInt32>.size)
            var address = AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: element)
            if AudioObjectGetPropertyData(device, &address, 0, nil, &size, &value) == noErr {
                return value
            }
        }
        return nil
    }
}
