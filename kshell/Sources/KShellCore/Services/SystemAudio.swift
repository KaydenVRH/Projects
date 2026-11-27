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

    /// Set the default output's volume (0…1), unmuting it when turned up.
    /// Devices without a main volume get each of their first two channels.
    public static func setVolume(_ volume: Float) {
        guard let device = defaultOutputDevice() else { return }
        var value = min(max(volume, 0), 1)
        let size = UInt32(MemoryLayout<Float>.size)
        var set = false
        for element in [kAudioObjectPropertyElementMain, AudioObjectPropertyElement(1), AudioObjectPropertyElement(2)] {
            var address = AudioObjectPropertyAddress(
                mSelector: kAudioDevicePropertyVolumeScalar,
                mScope: kAudioDevicePropertyScopeOutput,
                mElement: element
            )
            var settable: DarwinBoolean = false
            guard AudioObjectIsPropertySettable(device, &address, &settable) == noErr, settable.boolValue else { continue }
            if AudioObjectSetPropertyData(device, &address, 0, nil, size, &value) == noErr { set = true }
            // the main element covers every channel
            if set && element == kAudioObjectPropertyElementMain { break }
        }
        if set, value > 0 {
            var off = UInt32(0)
            var address = AudioObjectPropertyAddress(
                mSelector: kAudioDevicePropertyMute,
                mScope: kAudioDevicePropertyScopeOutput,
                mElement: kAudioObjectPropertyElementMain
            )
            _ = AudioObjectSetPropertyData(device, &address, 0, nil, UInt32(MemoryLayout<UInt32>.size), &off)
        }
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
