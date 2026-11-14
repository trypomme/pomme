// String searching as apps call it through Foundation (`range(of:options:)`,
// `replacingOccurrences`, `contains`): literal, case-insensitive, backwards,
// anchored, and regular-expression searches (`FOUND label result`).

import Darwin
import Foundation

func report(_ label: String, _ s: String, _ r: Range<String.Index>?) {
  let found = r.map { "\(s.distance(from: s.startIndex, to: $0.lowerBound)):\(s[$0])" } ?? "none"
  fputs("FOUND \(label) \(found)\n", stderr)
}

@main
struct StringSearch {
  static func main() {
    let hex = "ffa500FF"
    report("hex", hex, hex.range(of: #"^[0-9a-fA-F]{8}$"#, options: .regularExpression))
    report("digits", "x1y22", "x1y22".range(of: "[0-9]{2}", options: .regularExpression))
    report("nohex", "ffa50", "ffa50".range(of: #"^[0-9a-fA-F]{8}$"#, options: .regularExpression))
    report("caseless", "Hello", "Hello".range(of: "LL", options: .caseInsensitive))
    report("backwards", "a.b.c", "a.b.c".range(of: ".", options: .backwards))
    report("anchored", "abcabc", "abcabc".range(of: "bc", options: .anchored))
    report("literal", "a.b", "a.b".range(of: "."))
    fputs("FOUND replaced \("x1y22".replacingOccurrences(of: "[0-9]+", with: "#", options: .regularExpression))\n", stderr)
    // (Empty matches are replaced too, and the search moves past them.)
    fputs("FOUND empty \("ab".replacingOccurrences(of: "\\s*", with: "-", options: .regularExpression))\n", stderr)
    fputs("FOUND template \("John Smith".replacingOccurrences(of: "(\\w+) (\\w+)", with: "$2, $1", options: .regularExpression))\n", stderr)
    fputs("FOUND nonoverlapping \("aaa".replacingOccurrences(of: "aa", with: "b"))\n", stderr)
    fputs("FOUND contains \("abc".contains("bc")) \("abc".contains("x"))\n", stderr)
  }
}
