import SwiftUI

// GigCraft's look on iOS: system controls and layout, an amber accent, and a
// light touch of stage gear – values read at a glance sit in small display
// readouts, status shows as LEDs. The stage views are always dark.

extension Color {
    /// Chords: cool blue, readable on both themes, never confused with the accent.
    static let chord = Color("ChordColor")
    static let ledGreen = Color(red: 0.24, green: 0.84, blue: 0.48)
    static let screen = Color(red: 0.043, green: 0.047, blue: 0.055)
    static let screenText = Color(red: 1.0, green: 0.76, blue: 0.37)
}

/// A display readout: tiny engraved label, value in amber monospace.
struct Readout: View {
    let label: LocalizedStringKey
    let value: String
    var size: CGFloat = 17

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(label)
                .textCase(.uppercase)
                .font(.system(size: max(9, size * 0.45), weight: .semibold))
                .foregroundStyle(Color(white: 0.6))
            Text(verbatim: value)
                .font(.system(size: size, weight: .semibold, design: .monospaced))
                .foregroundStyle(Color.screenText)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(Color.screen, in: .rect(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.white.opacity(0.12)))
        .accessibilityElement(children: .combine)
    }
}

/// A status light with a soft halo.
struct LED: View {
    let color: Color

    var body: some View {
        ZStack {
            Circle().fill(color.opacity(0.25)).frame(width: 13, height: 13)
            Circle().fill(color).frame(width: 7, height: 7)
        }
    }
}

/// The main stage button: amber with dark text (white on amber is too weak).
struct StageButtonStyle: ButtonStyle {
    var prominent = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(prominent ? Color.black : Color.primary)
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .background(
                prominent ? Color.accentColor.opacity(configuration.isPressed ? 0.75 : 1) : Color.secondary.opacity(configuration.isPressed ? 0.3 : 0.18),
                in: .rect(cornerRadius: 12)
            )
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.white.opacity(prominent ? 0 : 0.1)))
    }
}
