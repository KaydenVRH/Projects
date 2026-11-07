import XCTest
@testable import KShellCore

final class BluetoothTests: XCTestCase {
    /// The name is the best signal: plenty of devices report a class of 0.
    func testKindsFromNames() {
        XCTAssertEqual(BluetoothDevice.kind(forName: "AirPods Pro"), .headphones)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Sony WH-1000XM5 Headphones"), .headphones)
        XCTAssertEqual(BluetoothDevice.kind(forName: "HomePod mini"), .speaker)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Magic Keyboard"), .keyboard)
        XCTAssertEqual(BluetoothDevice.kind(forName: "X11mouse2"), .mouse)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Magic Trackpad"), .mouse)
        XCTAssertEqual(BluetoothDevice.kind(forName: "DualSense Wireless Controller"), .controller)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Joy-Con (L)"), .controller)
        XCTAssertEqual(BluetoothDevice.kind(forName: "iPhone van kd"), .phone)
        XCTAssertEqual(BluetoothDevice.kind(forName: "iPad van kayden"), .tablet)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Apple Watch"), .watch)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Barev pen"), .pen)
        XCTAssertEqual(BluetoothDevice.kind(forName: "MacBook Pro"), .computer)
    }

    /// ...and when it isn't, the class of device decides: major in bits 8…12,
    /// minor in bits 2…7.
    func testKindsFromClassOfDevice() {
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x240418), .headphones)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x240404), .speaker)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x000100), .computer)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x000200), .phone)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x000540), .keyboard)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x000580), .mouse)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x000508), .controller)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0x000700), .watch)
        XCTAssertEqual(BluetoothDevice.kind(forName: "Unknown", classOfDevice: 0), .other)
    }

    func testConnectedDevicesSortFirstThenByName() {
        let devices = [
            BluetoothDevice(name: "Zebra", address: "c", kind: .other),
            BluetoothDevice(name: "AirPods", address: "a", kind: .headphones, isConnected: true),
            BluetoothDevice(name: "apple watch", address: "b", kind: .watch),
            BluetoothDevice(name: "Mouse", address: "d", kind: .mouse, isConnected: true),
        ]
        XCTAssertEqual(
            BluetoothDevice.sorted(devices).map(\.name),
            ["AirPods", "Mouse", "apple watch", "Zebra"]
        )
    }

    func testStatusText() {
        let on = BluetoothDevice(name: "AirPods", address: "a", kind: .headphones, isConnected: true)
        let off = BluetoothDevice(name: "AirPods", address: "a", kind: .headphones)
        XCTAssertEqual(on.status, "Connected")
        XCTAssertEqual(off.status, "Not connected")
    }
}
