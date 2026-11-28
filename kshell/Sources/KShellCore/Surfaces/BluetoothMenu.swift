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
    @State private var hovered: String?
    @FocusState private var focused: Bool
    @Namespace private var highlight

    private var bar: BarConfig { viewModel.appearance }
    private var palette: KitPalette { KitPalette(theme: theme, bar: bar) }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var sheet: SheetShape { SheetShape(radius: 22, corners: .leading) }
    private var devices: [BluetoothDevice] { controller.devices }

    var body: some View {
        VStack(spacing: 0) {
            header
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
        .onKeyPressOnce(.return) { toggleSelection(); return .handled }
        .onKeyPressOnce("p") { controller.togglePower(); return .handled }
        .onKeyPressOnce("r") { controller.refresh(); return .handled }
        .onKeyPressOnce("d") { toggleSelection(); return .handled }
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
        KitHeader(
            icon: "dot.radiowaves.left.and.right",
            title: "Bluetooth",
            subtitle: controller.isPowered
                ? "\(devices.filter(\.isConnected).count) connected · \(devices.count) paired"
                : "Radio off",
            palette: palette
        ) {
            KitToggle(isOn: controller.isPowered, palette: palette) { controller.togglePower() }
                .help("Turn Bluetooth \(controller.isPowered ? "off" : "on") (p)")
        }
    }

    private var footer: some View {
        VStack(spacing: 0) {
            if let message = controller.message {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message).lineLimit(2).fixedSize(horizontal: false, vertical: true)
                }
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(.orange)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            KitFooter(
                hints: [("⏎", "connect"), ("p", "power"), ("r", "refresh")],
                palette: palette
            ) { EmptyView() }
        }
        .animation(.kLayout, value: controller.message)
    }

    private var empty: some View {
        KitEmpty(
            icon: controller.isPowered ? "dot.radiowaves.left.and.right" : "bolt.horizontal.circle",
            title: controller.isPowered ? "No paired devices" : "Bluetooth is off",
            detail: controller.isPowered
                ? "Pair a device in System Settings → Bluetooth and it appears here."
                : "Press p, or flip the switch, to turn it back on.",
            palette: palette
        )
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
                                withAnimation(.kSelect) { selected = index }
                                controller.toggle(device)
                            }
                            .cascade(index, from: .trailing)
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
                withAnimation(.kLayout) {
                    proxy.scrollTo(devices[newValue].id, anchor: .center)
                }
            }
        }
    }

    private func row(_ device: BluetoothDevice, selected: Bool) -> some View {
        HStack(spacing: 12) {
            IconBubble(symbol: symbol(for: device), active: device.isConnected, palette: palette)

            VStack(alignment: .leading, spacing: 3) {
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
        .padding(.horizontal, 10)
        .padding(.vertical, 9)
        .kitCard(selected: selected, hovered: hovered == device.id, palette: palette, highlight: highlight)
        .contentShape(Rectangle())
        .onHover { inside in hovered = inside ? device.id : (hovered == device.id ? nil : hovered) }
    }

    @ViewBuilder
    private func trailing(for device: BluetoothDevice) -> some View {
        if controller.busy == device.address {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.7)
                .frame(width: 22, height: 22)
                .transition(.scale.combined(with: .opacity))
        } else {
            StatusPill(text: device.isConnected ? "On" : "Off", lit: device.isConnected, palette: palette)
                .transition(.scale.combined(with: .opacity))
        }
    }

    private func symbol(for device: BluetoothDevice) -> String {
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
    }

    // MARK: - keys

    private func vimMove(_ delta: Int) -> KeyPress.Result {
        guard !devices.isEmpty else { return .handled }
        withAnimation(.kSelect) { selected = max(0, min(devices.count - 1, selected + delta)) }
        return .handled
    }

    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        guard !devices.isEmpty else { return .handled }
        withAnimation(.kSelect) { selected = edge == .first ? 0 : devices.count - 1 }
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
