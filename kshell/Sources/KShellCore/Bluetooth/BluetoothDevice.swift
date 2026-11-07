import Foundation

/// One paired Bluetooth device, as the menu shows it.
public struct BluetoothDevice: Identifiable, Equatable, Sendable {
    public enum Kind: String, Sendable {
        case headphones, speaker, keyboard, mouse, controller, pen
        case phone, tablet, watch, computer, other
    }

    public var id: String { address }
    public let name: String
    public let address: String
    public let kind: Kind
    public let isPaired: Bool
    public var isConnected: Bool

    public init(
        name: String,
        address: String,
        kind: Kind,
        isPaired: Bool = true,
        isConnected: Bool = false
    ) {
        self.name = name
        self.address = address
        self.kind = kind
        self.isPaired = isPaired
        self.isConnected = isConnected
    }

    /// What the second line says.
    public var status: String {
        isConnected ? "Connected" : "Not connected"
    }

    /// Which glyph a device gets.
    ///
    /// The name decides first: plenty of devices report a class of `0` (the
    /// mouse and the Watch here do), and "AirPods" is a better answer than
    /// "audio device" anyway. The Bluetooth class is the fallback — major device
    /// class in bits 8…12, minor in bits 2…7.
    public static func kind(forName name: String, classOfDevice: UInt32 = 0) -> Kind {
        let lower = name.lowercased()
        func has(_ words: [String]) -> Bool { words.contains { lower.contains($0) } }

        if has(["airpod", "headphone", "headset", "beats", "earbud", "buds "]) { return .headphones }
        if has(["speaker", "homepod", "soundbar", "boom", "sonos"]) { return .speaker }
        if has(["keyboard", "keychron"]) { return .keyboard }
        if has(["mouse", "trackpad", "track pad", "mx master", "magic track"]) { return .mouse }
        if has(["controller", "joy-con", "joycon", "dualsense", "dualshock", "xbox", "gamepad", "switch pro"]) {
            return .controller
        }
        if has(["iphone", "phone"]) { return .phone }
        if has(["ipad", "tablet"]) { return .tablet }
        if has(["watch"]) { return .watch }
        if has(["macbook", "imac", "mac mini", "mac studio", "mac pro"]) { return .computer }
        if has(["pen", "pencil", "stylus"]) { return .pen }

        let major = (classOfDevice >> 8) & 0x1F
        let minor = (classOfDevice >> 2) & 0x3F
        switch major {
        case 1: return .computer            // computer
        case 2: return .phone               // phone
        case 4:                             // audio / video
            return minor == 0x06 ? .headphones : .speaker
        case 5:                             // peripheral
            switch minor {
            case 0x10: return .keyboard     // keyboard
            case 0x20, 0x30: return .mouse      // pointing, combo
            case 0x02, 0x08: return .controller // gamepad/joystick
            default: return .other
            }
        case 7: return .watch               // wearable
        default: return .other
        }
    }

    /// Connected devices first, then by name — a menu that reorders itself every
    /// poll is hard to use, so this is the one order everywhere.
    public static func sorted(_ devices: [BluetoothDevice]) -> [BluetoothDevice] {
        devices.sorted { left, right in
            if left.isConnected != right.isConnected { return left.isConnected }
            let comparison = left.name.localizedCaseInsensitiveCompare(right.name)
            if comparison != .orderedSame { return comparison == .orderedAscending }
            return left.address < right.address
        }
    }
}
