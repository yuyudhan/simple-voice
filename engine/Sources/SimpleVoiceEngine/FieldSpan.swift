// FilePath: engine/Sources/SimpleVoiceEngine/FieldSpan.swift
// Keeps track of where a pasted dictation sits in a text field while the user edits the field.

import Foundation

/// Where the pasted text sits in a field's value, in UTF-16 code units (the unit Accessibility
/// ranges use), kept in place as the value is edited.
struct FieldSpan {
    private var units: [UInt16]
    private var start: Int
    private var end: Int

    /// Prefers the occurrence that ends at the caret, where a paste leaves it; otherwise the
    /// last one, since dictation is usually appended.
    init?(value: String, needle: String, caret: Int?) {
        let field = value as NSString
        let length = (needle as NSString).length
        var found = NSRange(location: NSNotFound, length: 0)
        if let caret, caret >= length, caret <= field.length {
            let beforeCaret = NSRange(location: caret - length, length: length)
            found = field.range(of: needle, options: .literal, range: beforeCaret)
        }
        if found.location == NSNotFound {
            found = field.range(of: needle, options: [.literal, .backwards])
        }
        guard found.location != NSNotFound else {
            return nil
        }
        units = Array(value.utf16)
        start = found.location
        end = found.location + found.length
    }

    var text: String {
        String(decoding: units[start..<end], as: UTF16.self)
    }

    var isEmpty: Bool {
        start == end
    }

    /// Treats the difference between the old and new value as one replaced run: the common
    /// prefix and suffix are unchanged, so only a run touching the span can change its text.
    mutating func update(to value: String) {
        let next = Array(value.utf16)
        let limit = min(units.count, next.count)
        var prefix = 0
        while prefix < limit, units[prefix] == next[prefix] {
            prefix += 1
        }
        var suffix = 0
        while suffix < limit - prefix, units[units.count - 1 - suffix] == next[next.count - 1 - suffix] {
            suffix += 1
        }
        let oldEnd = units.count - suffix
        let delta = next.count - units.count
        let before = oldEnd < start || (oldEnd == start && prefix < start)
        let after = prefix > end || (prefix == end && oldEnd > end)
        if before {
            start += delta
            end += delta
        } else if !after {
            // An insertion right at either edge joins the span: it is typed into the dictation.
            start = min(start, prefix)
            end = oldEnd > end ? next.count - suffix : end + delta
        }
        units = next
    }
}
