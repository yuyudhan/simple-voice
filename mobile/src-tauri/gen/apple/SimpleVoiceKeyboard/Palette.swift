// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/Palette.swift
// Signal colours from src/styles/tokens.css, the same values as the Android keyboard's
// Palette.kt; keep all three in step. The keyboard is a surface with keys lifted one step above it.

import UIKit

struct Palette {
    /// Keyboard background: light `--surface`, dark `--bg`.
    let background: UIColor
    /// Letter keys: light `--panel`, dark `--surface-hover`.
    let key: UIColor
    /// Shift, backspace, layer and return keys: light `--surface-hover`, dark `--border-strong`.
    let functionKey: UIColor
    let border: UIColor
    let text: UIColor
    let muted: UIColor
    let accent: UIColor
    let onAccent: UIColor
    let success: UIColor
    let warning: UIColor
    let danger: UIColor

    static func of(_ traits: UITraitCollection) -> Palette {
        traits.userInterfaceStyle == .dark ? dark : light
    }

    private static let light = Palette(
        background: rgb(0xF4F4F5),
        key: rgb(0xFFFFFF),
        functionKey: rgb(0xEBEBED),
        border: rgb(0xE6E6E9),
        text: rgb(0x151518),
        muted: rgb(0x62626B),
        accent: rgb(0xF0521A),
        onAccent: rgb(0xFFFFFF),
        success: rgb(0x1C7F47),
        warning: rgb(0x8F6200),
        danger: rgb(0xC42639)
    )

    private static let dark = Palette(
        background: rgb(0x111113),
        key: rgb(0x222226),
        functionKey: rgb(0x3A3A41),
        border: rgb(0x29292E),
        text: rgb(0xEDEDF0),
        muted: rgb(0xA2A2AB),
        accent: rgb(0xFF6A2E),
        onAccent: rgb(0x1A0B04),
        success: rgb(0x5FD08F),
        warning: rgb(0xF0C05A),
        danger: rgb(0xFF6B7A)
    )

    private static func rgb(_ hex: UInt32) -> UIColor {
        UIColor(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: 1
        )
    }
}
