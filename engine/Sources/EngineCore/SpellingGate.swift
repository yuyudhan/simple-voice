// FilePath: engine/Sources/EngineCore/SpellingGate.swift
// Which of the dictionary booster's replacements the helper accepts. FluidAudio's rescorer swaps
// transcript words for a dictionary term wherever the CTC spotter scores the term well enough,
// and on its own it also writes terms over words that only share a rhythm with them.

/// The spelling check every booster replacement must pass before it reaches the transcript.
public enum SpellingGate {
    /// The rescorer accepts a term spelled only half like the words it replaces; on real
    /// recordings it wrote a team's product name over "Press control" and a five-letter name
    /// over "Kafka". A replacement must spell at least this close to what it replaces.
    public static let minSpellingSimilarity = 0.75

    /// A word TDT ran together from two names ("Ajvarhitanshu") must not collapse into one of
    /// them (a replacement half again shorter would drop a spoken word), and a replacement must
    /// spell close to the words it replaces.
    public static func keepsEveryWord(_ original: String, replacedBy replacement: String) -> Bool {
        let letters = { (text: String) in Array(text.lowercased().filter { $0.isLetter || $0.isNumber }) }
        let from = letters(original)
        let to = letters(replacement)
        guard !from.isEmpty, !to.isEmpty, from.count * 2 <= to.count * 3 else { return false }
        let longer = Double(max(from.count, to.count))
        return 1 - Double(editDistance(from, to)) / longer >= minSpellingSimilarity
    }

    private static func editDistance(_ a: [Character], _ b: [Character]) -> Int {
        var row = Array(0...b.count)
        for (i, x) in a.enumerated() {
            var diagonal = row[0]
            row[0] = i + 1
            for (j, y) in b.enumerated() {
                let substitution = diagonal + (x == y ? 0 : 1)
                diagonal = row[j + 1]
                row[j + 1] = min(substitution, row[j] + 1, row[j + 1] + 1)
            }
        }
        return row[b.count]
    }
}
