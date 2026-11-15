import XCTest
@testable import Tracker

final class GeometryTests: XCTestCase {
    func testLetterboxedVideoUsesSourceCoordinates() {
        let source = CGSize(width: 1920, height: 1080)
        let rect = VideoGeometry.fittedRect(source: source, container: CGSize(width: 600, height: 600))
        XCTAssertEqual(rect.height, 337.5)
        XCTAssertNil(VideoGeometry.sourcePoint(CGPoint(x: 300, y: 10), source: source, rect: rect))
        let point = VideoGeometry.sourcePoint(CGPoint(x: 300, y: 300), source: source, rect: rect)
        XCTAssertEqual(point?.x, 960)
        XCTAssertEqual(point?.y, 540)
    }

    func testDegenerateCalibrationIsRejected() {
        XCTAssertFalse(Calibration(first: .zero, second: .zero, length: 1, unit: "m").isValid)
        XCTAssertFalse(Calibration(first: .zero, second: CGPoint(x: 100, y: 0), length: 0, unit: "m").isValid)
        let valid = Calibration(first: .zero, second: CGPoint(x: 30, y: 40), length: 0.5, unit: "m")
        XCTAssertTrue(valid.isValid)
        XCTAssertEqual(valid.pixelsPerUnit, 100)
    }
}
