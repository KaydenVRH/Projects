import Combine
import Foundation
import IOBluetooth
import ObjectiveC

/// Reads and drives the Mac's Bluetooth through `IOBluetooth`.
///
/// Reading is cheap (`pairedDevices()` is a list lookup), so a poll can run while
/// the menu is open and nowhere else. Connecting is not: `openConnection()` blocks
/// until the device answers or the page times out, so actions run off the main
/// thread and the menu shows which row is busy.
///
/// No helper binary is involved — `IOBluetooth` is the same framework the system's
/// own Bluetooth menu uses, and it needs no special permission for this.
public final class BluetoothController: ObservableObject {
    @Published public private(set) var devices: [BluetoothDevice] = []
    @Published public private(set) var isPowered = true
    /// False when the framework cannot be reached at all (no controller).
    @Published public private(set) var isAvailable = true
    /// The address being switched, so its row can show a spinner.
    @Published public private(set) var busy: String?
    /// The last thing that went wrong, shown in the panel until the next action.
    @Published public private(set) var message: String?

    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "kshell.bluetooth", qos: .userInitiated)

    public init() {}

    // MARK: - reading

    public func refresh() {
        guard Thread.isMainThread else {
            DispatchQueue.main.async { [weak self] in self?.refresh() }
            return
        }
        let controller = IOBluetoothHostController.default()
        let powered = controller?.powerState.rawValue == 1
        let paired = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice] ?? []

        let mapped: [BluetoothDevice] = paired.compactMap { device -> BluetoothDevice? in
            guard let address = device.addressString, !address.isEmpty else { return nil }
            let name = (device.nameOrAddress ?? "").isEmpty ? address : device.nameOrAddress!
            return BluetoothDevice(
                name: name,
                address: address,
                kind: BluetoothDevice.kind(forName: name, classOfDevice: device.classOfDevice),
                isPaired: true,
                isConnected: device.isConnected()
            )
        }

        isAvailable = controller != nil
        isPowered = powered
        // While Bluetooth is off the paired list is stale; don't claim anything
        // is connected.
        devices = BluetoothDevice.sorted(mapped.map { device in
            var device = device
            if !powered { device.isConnected = false }
            return device
        })
    }

    /// Poll while the menu is open, so a device connecting from its own side (or
    /// dropping out of range) shows up without a manual refresh.
    public func startPolling(interval: TimeInterval = 2) {
        refresh()
        guard timer == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now() + interval, repeating: interval, leeway: .milliseconds(250))
        timer.setEventHandler { [weak self] in self?.refresh() }
        self.timer = timer
        timer.resume()
    }

    public func stopPolling() {
        timer?.cancel()
        timer = nil
        message = nil
    }

    // MARK: - acting

    public func toggle(_ device: BluetoothDevice) {
        guard busy == nil else { return }
        busy = device.address
        message = nil
        let connect = !device.isConnected

        queue.async { [weak self] in
            guard let self else { return }
            let paired = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice] ?? []
            guard let target = paired.first(where: { $0.addressString == device.address }) else {
                self.finish("\(device.name) is no longer paired.")
                return
            }

            let result = connect ? target.openConnection() : target.closeConnection()
            // The connection state settles a moment after the call returns.
            Thread.sleep(forTimeInterval: 0.5)
            let connected = target.isConnected()
            DispatchQueue.main.async {
                self.busy = nil
                self.refresh()
                if result != kIOReturnSuccess {
                    self.message = Self.describe(result, name: device.name, connecting: connect)
                } else if connected != connect {
                    self.message = connect
                        ? "\(device.name) did not connect — wake it and try again."
                        : "\(device.name) is still connected."
                }
            }
        }
    }

    public func togglePower() {
        guard busy == nil else { return }
        busy = "power"
        message = nil
        let on = !isPowered
        queue.async { [weak self] in
            guard let self else { return }
            Self.setPower(on)
            Thread.sleep(forTimeInterval: 1.0)
            DispatchQueue.main.async {
                self.busy = nil
                self.refresh()
                if self.isPowered != on {
                    self.message = on ? "Bluetooth did not turn on." : "Bluetooth did not turn off."
                }
            }
        }
    }

    // MARK: - helpers

    /// `IOBluetoothHostController` has `setPowerState:` but Swift does not import
    /// it, and the C function blueutil uses is not in the SDK headers either. The
    /// selector is the public class's own, reached through the runtime and only
    /// when the class still answers to it — the fallback is doing nothing, which
    /// the refresh then reports as "did not turn on".
    private static func setPower(_ on: Bool) {
        guard let controller = IOBluetoothHostController.default() else { return }
        let selector = NSSelectorFromString("setPowerState:")
        guard controller.responds(to: selector),
              let method = class_getInstanceMethod(object_getClass(controller), selector) else {
            return
        }
        typealias Setter = @convention(c) (AnyObject, Selector, Int32) -> Void
        let implementation = unsafeBitCast(method_getImplementation(method), to: Setter.self)
        implementation(controller, selector, on ? 1 : 0)
    }

    private func finish(_ message: String) {
        DispatchQueue.main.async { [weak self] in
            self?.busy = nil
            self?.message = message
        }
    }

    private static func describe(_ result: IOReturn, name: String, connecting: Bool) -> String {
        let verb = connecting ? "connect to" : "disconnect"
        switch result {
        case kIOReturnTimeout:
            return "\(name) did not answer — it may be asleep or out of range."
        case kIOReturnNotPermitted:
            return "Bluetooth access was refused for kshell."
        case kIOReturnNoPower:
            return "Bluetooth is off."
        case kIOReturnNoDevice:
            return "\(name) is not available."
        default:
            return "Could not \(verb) \(name) (0x\(String(UInt32(bitPattern: result), radix: 16)))."
        }
    }
}
