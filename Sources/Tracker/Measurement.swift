import Foundation
import CoreGraphics

struct Measurement: Identifiable {
    let id = UUID()
    let time: Double
    let pixel: CGPoint
}

struct PointTrack: Identifiable {
    let id = UUID()
    var name: String
    var points: [Measurement] = []
}

struct Calibration {
    var first: CGPoint
    var second: CGPoint
    var length: Double
    var unit: String

    var pixelsPerUnit: Double { hypot(second.x - first.x, second.y - first.y) / length }
    var isValid: Bool { length.isFinite && length > 0 && pixelsPerUnit.isFinite && pixelsPerUnit > 0 }
}

enum VideoGeometry {
    static func fittedRect(source: CGSize, container: CGSize) -> CGRect {
        guard source.width > 0, source.height > 0 else { return .zero }
        let scale = min(container.width / source.width, container.height / source.height)
        let size = CGSize(width: source.width * scale, height: source.height * scale)
        return CGRect(x: (container.width - size.width) / 2,
                      y: (container.height - size.height) / 2, width: size.width, height: size.height)
    }

    static func sourcePoint(_ point: CGPoint, source: CGSize, rect: CGRect) -> CGPoint? {
        guard rect.width > 0, rect.height > 0, rect.contains(point) else { return nil }
        return CGPoint(x: (point.x - rect.minX) / rect.width * source.width,
                       y: (point.y - rect.minY) / rect.height * source.height)
    }
}
