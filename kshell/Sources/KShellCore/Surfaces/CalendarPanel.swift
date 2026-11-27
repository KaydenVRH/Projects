import AppKit
import SwiftUI

/// The calendar: the month, today lit, a big clock with the day's progress
/// under it. Opened by clicking the clock (or `kshell trigger calendar_toggle`);
/// ←/→ or h/l step through months, t comes back to today.
final class CalendarPanel {
    private let overlay: OverlayLauncher

    init(viewModel: BarViewModel, margin: CGFloat) {
        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .trailing,
            margin: margin,
            size: { screen in
                NSSize(width: min(screen.frame.width * 0.3, 380), height: min(screen.frame.height * 0.7, 540))
            },
            content: { onClose in
                AnyView(CalendarView(viewModel: viewModel, onClose: onClose))
            }
        )
    }

    func toggle() { overlay.toggle() }
    func show() { overlay.show() }
    func hide() { overlay.hide() }
}

struct CalendarView: View {
    @ObservedObject var viewModel: BarViewModel
    let onClose: () -> Void

    /// Months away from this one.
    @State private var offset = 0
    @State private var direction: Edge = .trailing
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var palette: KitPalette { KitPalette(theme: viewModel.theme, bar: bar) }
    private var calendar: Calendar { .autoupdatingCurrent }

    private var month: Date {
        let start = calendar.dateInterval(of: .month, for: Date())?.start ?? Date()
        return calendar.date(byAdding: .month, value: offset, to: start) ?? start
    }

    var body: some View {
        VStack(spacing: 0) {
            KitHeader(icon: "calendar", title: month.formatted(.dateTime.month(.wide).year()),
                      subtitle: subtitle, palette: palette) {
                HStack(spacing: 6) {
                    arrow("chevron.left") { step(-1) }
                    arrow("chevron.right") { step(1) }
                }
            }
            grid
                .id(offset)
                .transition(.asymmetric(
                    insertion: .move(edge: direction).combined(with: .opacity),
                    removal: .opacity
                ))
                .padding(.horizontal, 14)
                .padding(.top, 6)
            Spacer(minLength: 0)
            CalendarClock(palette: palette)
                .padding(.horizontal, 18)
                .padding(.bottom, 12)
            KitFooter(hints: [("←→", "month"), ("t", "today"), ("esc", "close")], palette: palette) { EmptyView() }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(PanelSurface(viewModel: viewModel, edge: .trailing, fallback: SheetShape(radius: 22, corners: .leading)))
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.leftArrow) { step(-1) }
        .onKeyPress(.rightArrow) { step(1) }
        .onKeyPress("h", phases: .down) { _ in step(-1) }
        .onKeyPress("l", phases: .down) { _ in step(1) }
        .onKeyPress("t", phases: .down) { _ in
            direction = offset > 0 ? .leading : .trailing
            withAnimation(.kLayout) { offset = 0 }
            return .handled
        }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear { focused = true }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            focused = true
        }
    }

    private var subtitle: String {
        let today = Date()
        let week = calendar.component(.weekOfYear, from: today)
        let day = calendar.ordinality(of: .day, in: .year, for: today) ?? 0
        return "\(today.formatted(.dateTime.weekday(.wide).day().month(.wide))) · week \(week) · day \(day)"
    }

    private func step(_ by: Int) -> KeyPress.Result {
        direction = by > 0 ? .trailing : .leading
        withAnimation(.kLayout) { offset += by }
        return .handled
    }

    private func arrow(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .bold))
                .foregroundColor(palette.foreground.opacity(0.8))
                .frame(width: 24, height: 24)
                .background(dayShape.fill(palette.foreground.opacity(0.08)))
        }
        .buttonStyle(.plain)
        .pressBounce()
    }

    // MARK: - the month

    /// The days to show: whole weeks, from the week the month starts in.
    private var days: [Date] {
        guard let interval = calendar.dateInterval(of: .month, for: month) else { return [] }
        let first = interval.start
        let lead = (calendar.component(.weekday, from: first) - calendar.firstWeekday + 7) % 7
        let start = calendar.date(byAdding: .day, value: -lead, to: first) ?? first
        let count = calendar.range(of: .day, in: .month, for: month)?.count ?? 30
        let weeks = Int(ceil(Double(lead + count) / 7))
        return (0..<(weeks * 7)).compactMap { calendar.date(byAdding: .day, value: $0, to: start) }
    }

    private var weekdays: [String] {
        let symbols = calendar.veryShortStandaloneWeekdaySymbols
        let first = calendar.firstWeekday - 1
        return Array(symbols[first...] + symbols[..<first])
    }

    private var dayShape: AnyShape {
        if palette.neon { return AnyShape(SlantShape(slant: 5)) }
        if palette.hud { return AnyShape(ChamferShape(cut: 5)) }
        return AnyShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private var grid: some View {
        let columns = Array(repeating: GridItem(.flexible(), spacing: 4), count: 7)
        return LazyVGrid(columns: columns, spacing: 5) {
            ForEach(weekdays.indices, id: \.self) { index in
                Text(palette.neon || palette.hud ? weekdays[index].uppercased() : weekdays[index])
                    .font(.custom(bar.fontFamily, size: 10.5).weight(.bold))
                    .foregroundColor(palette.highlight.opacity(0.75))
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 3)
            }
            // Keyed by the date: the weekday names above use 0…6, and a grid
            // drops views whose ids repeat.
            ForEach(Array(days.enumerated()), id: \.element) { index, day in
                cell(day)
                    .cascade(index / 7, from: direction)
            }
        }
    }

    private func cell(_ day: Date) -> some View {
        let inMonth = calendar.isDate(day, equalTo: month, toGranularity: .month)
        let today = calendar.isDateInToday(day)
        let weekend = calendar.isDateInWeekend(day)
        return Text("\(calendar.component(.day, from: day))")
            .font(.custom(bar.fontFamily, size: 13).weight(today ? .black : .regular))
            .foregroundColor(today ? .black.opacity(0.85)
                             : palette.foreground.opacity(inMonth ? (weekend ? 0.6 : 0.92) : 0.25))
            .frame(maxWidth: .infinity, minHeight: 32)
            .background {
                if today {
                    dayShape.fill(palette.accent)
                        .shadow(color: palette.accent.opacity(0.8), radius: 6)
                        .overlay {
                            if palette.neon {
                                dayShape.stroke(palette.highlight, lineWidth: 1.1).offset(x: 2, y: -1.5)
                            }
                        }
                }
            }
    }
}

/// A big clock with seconds and a line under it filling as the day goes.
private struct CalendarClock: View {
    let palette: KitPalette

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let now = context.date
            let start = Calendar.autoupdatingCurrent.startOfDay(for: now)
            let progress = now.timeIntervalSince(start) / 86_400
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline, spacing: 4) {
                    let time = now.formatted(.dateTime.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
                    if palette.neon {
                        ChromaText(text: time, font: .system(size: 40, weight: .black).italic().monospacedDigit(),
                                   magenta: palette.accent, cyan: palette.highlight, foreground: palette.foreground)
                    } else {
                        Text(time)
                            .font(.system(size: 40, weight: .bold).monospacedDigit())
                            .foregroundColor(palette.foreground)
                    }
                    Text(now.formatted(.dateTime.second(.twoDigits)))
                        .font(.system(size: 16, weight: .bold).monospacedDigit())
                        .foregroundColor(palette.highlight)
                        .contentTransition(.numericText())
                    Spacer()
                    Text("\(Int(progress * 100))% of today")
                        .font(.custom(palette.font, size: 10.5))
                        .foregroundColor(palette.dim)
                }
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule().fill(palette.foreground.opacity(0.1))
                        Capsule()
                            .fill(LinearGradient(colors: [palette.accent, palette.highlight],
                                                 startPoint: .leading, endPoint: .trailing))
                            .frame(width: geo.size.width * progress)
                            .shadow(color: palette.accent.opacity(0.7), radius: 4)
                    }
                }
                .frame(height: 4)
            }
        }
    }
}
