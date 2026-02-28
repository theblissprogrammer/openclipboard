import XCTest
import OpenClipboard

final class PairingHelpersTests: XCTestCase {
    func testNormalizeQrStringTrimsWhitespaceAndNewlines() {
        let s = "  openclipboard://pair?foo=bar\n"
        XCTAssertEqual(PairingHelpers.normalizeQrString(s), "openclipboard://pair?foo=bar")
    }
}
