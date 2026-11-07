import AppKit
import SwiftUI

/// The Bluetooth menu: the paired devices, a click or `⏎` to connect or
/// disconnect one, and the radio's power switch.
///
/// A trailing sidebar like the theme and wallpaper menus, so it grows out of the
/// screen edge with the same motion, borrows the frame's glass and carries the
/// same concave fillets where it meets it.
final class BluetoothMenu {
    private let viewModel: BarViewModel
    private let controller = BluetoothController()
    private let overlay: OverlayLauncher

    init(viewModel: BarViewModel, margin: CGFloat) {
        self.viewModel = viewModel
        let controller = self.controller
        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .trailing,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.30, 420),
                    height: min(screen.frame.height * 0.70, 620)
                )
            },
            content: { onClose in
                AnyView(BluetoothMenuView(
                    viewModel: viewModel,
                    controller: controller,
                    onClose: onClose
                ))
            }
        )
        // Poll only while it is on screen: the list is cheap, but a menu that is
        // closed has nothing to keep up to date.
        overlay.onShow = { [weak self] in self?.controller.startPolling() }
        overlay.onHide = { [weak self] in self?.controller.stopPolling() }
    }

    func toggle() { overlay.toggle() }
    func show(animated: Bool = true) { overlay.show(animated: animated) }
    func hide(animated: Bool = true) { overlay.hide(animated: animated) }
    var visible: Bool { overlay.visible }

    /// Connect or disconnect by name or address — for `kshell trigger`.
    func toggleDevice(matching query: String) {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return }
        controller.refresh()
        guard let device = controller.devices.first(where: {
            $0.address.lowercased() == needle || $0.name.lowercased().contains(needle)
        }) else { return }
        controller.toggle(device)
    }

    /// `on`, `off` or anything else for a toggle.
    func setPower(_ value: String) {
        switch value.lowercased() {
        case "on": if !controller.isPowered { controller.togglePower() }
        case "off": if controller.isPowered { controller.togglePower() }
        default: controller.togglePower()
        }
    }
}

struct BluetoothMenuView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var controller: BluetoothController
    let onClose: () -> Void

    @State private var selected = 0
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var sheet: SheetShape { SheetShape(radius: 22, corners: .leading) }
    private var devices: [BluetoothDevice] { controller.devices }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(accent.opacity(0.22))
            if devices.isEmpty {
                empty
            } else {
                list
            }
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(PanelSurface(viewModel: viewModel, edge: .trailing, fallback: sheet))
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .modifier(VimListKeys(move: vimMove, jump: jump))
        .onKeyPress(.upArrow) { vimMove(-1) }
        .onKeyPress(.downArrow) { vimMove(1) }
        .onKeyPress(.return) { toggleSelection(); return .handled }
        .onKeyPress("p", phases: .down) { _ in controller.togglePower(); return .handled }
        .onKeyPress("r", phases: .down) { _ in controller.refresh(); return .handled }
        .onKeyPress("d", phases: .down) { _ in toggleSelection(); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear {
            controller.refresh()
            refocus()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
        .onChange(of: devices.count) { _, _ in
            selected = min(selected, max(0, devices.count - 1))
        }
    }

    // MARK: - chrome

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "dot.radiowaves.left.and.right")
                .font(.system(size: 15, weight: .medium))
                .foregroundColor(accent)
            Text("Bluetooth")
                .font(.custom(bar.fontFamily, size: 18))
                .foregroundColor(foreground)
            Spacer()
            Button(action: { controller.togglePower() }) {
                HStack(spacing: 5) {
                    Image(systemName: "power")
                        .font(.system(size: 10, weight: .bold))
                    Text(controller.isPowered ? "On" : "Off")
                        .font(.custom(bar.fontFamily, size: 11))
                }
                .foregroundColor(controller.isPowered ? accent : dim)
                .padding(.horizontal, 9)
                .padding(.vertical, 4)
                .background(
                    Capsule().fill(
                        controller.isPowered
                            ? accent.opacity(0.16)
                            : Color.white.opacity(0.06)
                    )
                )
            }
            .buttonStyle(.plain)
            .help("Turn Bluetooth \(controller.isPowered ? "off" : "on") (p)")
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
    }

    private var footer: some View {
        VStack(spacing: 6) {
            Divider().overlay(accent.opacity(0.12))
            if let message = controller.message {
                Text(message)
                    .font(.custom(bar.fontFamily, size: 11))
                    .foregroundColor(.orange)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 10) {
                Text("\(devices.count) paired")
                    .font(.custom(bar.fontFamily, size: 11))
                    .foregroundColor(foreground.opacity(0.45))
                Spacer()
                Text("j/k  ·  ⏎ connect  ·  p power  ·  r refresh")
                    .font(.custom(bar.fontFamily, size: 10))
                    .foregroundColor(foreground.opacity(0.40))
            }
        }
        .padding(.horizontal, 18)
        .padding(.top, 8)
        .padding(.bottom, 10)
    }

    private var empty: some View {
        VStack(spacing: 9) {
            Image(systemName: controller.isPowered
                  ? "dot.radiowaves.left.and.right"
                  : "bolt.horizontal.circle")
                .font(.system(size: 26, weight: .thin))
                .foregroundColor(accent.opacity(0.65))
            Text(controller.isPowered ? "No paired devices" : "Bluetooth is off")
                .font(.custom(bar.fontFamily, size: 15))
                .foregroundColor(foreground)
            Text(controller.isPowered
                 ? "Pair a device in System Settings → Bluetooth and it appears here."
                 : "Press p to turn it back on.")
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(foreground.opacity(0.45))
                .multilineTextAlignment(.center)
                .frame(maxWidth: 260)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(30)
    }

    // MARK: - the list

    private var list: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 8) {
                    ForEach(Array(devices.enumerated()), id: \.element.id) { index, device in
                        row(device, selected: index == selected)
                            .id(device.id)
                            .onTapGesture {
                                selected = index
                                controller.toggle(device)
                            }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.viewAligned)
            .frame(maxHeight: .infinity)
            .onChange(of: selected) { _, newValue in
                guard devices.indices.contains(newValue) else { return }
                withAnimation(.snappy(duration: 0.26, extraBounce: 0.24)) {
                    proxy.scrollTo(devices[newValue].id, anchor: .center)
                }
            }
        }
    }

    private func row(_ device: BluetoothDevice, selected: Bool) -> some View {
        HStack(spacing: 12) {
            icon(for: device)
                .frame(width: 26, height: 26)
                .foregroundColor(device.isConnected ? accent : foreground.opacity(0.72))

            VStack(alignment: .leading, spacing: 2) {
                Text(device.name)
                    .font(.custom(bar.fontFamily, size: 13.5))
                    .foregroundColor(foreground)
                    .lineLimit(1)
                Text(device.status)
                    .font(.custom(bar.fontFamily, size: 10.5))
                    .foregroundColor(device.isConnected ? accent.opacity(0.9) : dim)
            }

            Spacer(minLength: 8)

            trailing(for: device)
        }
        .padding(.horizontal, 11)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(selected ? accent.opacity(0.14) : Color.white.opacity(0.025))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(selected ? accent.opacity(0.55) : Color.clear, lineWidth: 1)
        )
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private func trailing(for device: BluetoothDevice) -> some View {
        if controller.busy == device.address {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.7)
                .frame(width: 22, height: 22)
        } else if device.isConnected {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 15))
                .foregroundColor(accent)
        } else {
            Text("Off")
                .font(.custom(bar.fontFamily, size: 10))
                .foregroundColor(dim.opacity(0.8))
                .padding(.horizontal, 7)
                .padding(.vertical, 3)
                .background(Capsule().fill(Color.white.opacity(0.06)))
        }
    }

    @ViewBuilder
    private func icon(for device: BluetoothDevice) -> some View {
        let symbol: String = {
            switch device.kind {
            case .headphones: return "headphones"
            case .speaker: return "hifispeaker.fill"
            case .keyboard: return "keyboard"
            case .mouse: return "computermouse.fill"
            case .controller: return "gamecontroller.fill"
            case .pen: return "pencil.tip"
            case .phone: return "iphone"
            case .tablet: return "ipad"
            case .watch: return "applewatch"
            case .computer: return "desktopcomputer"
            case .other: return "dot.radiowaves.left.and.right"
            }
        }()
        Image(systemName: symbol)
            .font(.system(size: 15, weight: .regular))
    }

    // MARK: - keys

    private func vimMove(_ delta: Int) -> KeyPress.Result {
        guard !devices.isEmpty else { return .ignored }
        selected = max(0, min(devices.count - 1, selected + delta))
        return .handled
    }

    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        guard !devices.isEmpty else { return .ignored }
        selected = edge == .first ? 0 : devices.count - 1
        return .handled
    }

    private func toggleSelection() {
        guard devices.indices.contains(selected) else { return }
        controller.toggle(devices[selected])
    }

    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}
