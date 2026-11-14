import AVFoundation
import CoreImage

struct DecodedFrame {
    let stamp: FrameStamp
    let image: CGImage
    var size: CGSize { CGSize(width: image.width, height: image.height) }
    func grayscale() throws -> GrayImage {
        var bytes = [UInt8](repeating: 0, count: image.width * image.height)
        let rendered = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height,
                                          bitsPerComponent: 8, bytesPerRow: image.width,
                                          space: CGColorSpaceCreateDeviceGray(), bitmapInfo: 0) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        guard rendered else { throw MediaError.decode }
        return GrayImage(width: image.width, height: image.height, pixels: bytes)
    }
}

enum MediaError: LocalizedError {
    case decode, noFrames
    var errorDescription: String? {
        switch self {
        case .decode: "The requested video frame could not be decoded."
        case .noFrames: "No readable video frames were found."
        }
    }
}

// One reader retains at most its current sample and the consumer's current image.
final class SequentialFrames {
    private let reader: AVAssetReader
    private let output: AVAssetReaderTrackOutput
    private let transform: CGAffineTransform
    private let context = CIContext(options: [.cacheIntermediates: false])

    init(url: URL, start: CMTime = .zero) async throws {
        let asset = AVURLAsset(url: url)
        guard let track = try await asset.loadTracks(withMediaType: .video).first else { throw MediaError.noFrames }
        transform = try await track.load(.preferredTransform)
        reader = try AVAssetReader(asset: asset)
        output = AVAssetReaderTrackOutput(track: track, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        output.alwaysCopiesSampleData = false
        reader.add(output)
        if start > .zero { reader.timeRange = CMTimeRange(start: start, duration: .positiveInfinity) }
        guard reader.startReading() else { throw reader.error ?? MediaError.decode }
    }

    func next() throws -> DecodedFrame? {
        try Task.checkCancellation()
        guard let sample = output.copyNextSampleBuffer() else {
            if reader.status == .failed { throw reader.error ?? MediaError.decode }
            return nil
        }
        guard let buffer = CMSampleBufferGetImageBuffer(sample) else { throw MediaError.decode }
        let stamp = CMSampleBufferGetPresentationTimeStamp(sample)
        let ci = CIImage(cvPixelBuffer: buffer).transformed(by: transform)
        guard let image = context.createCGImage(ci, from: ci.extent) else { throw MediaError.decode }
        return DecodedFrame(stamp: FrameStamp(value: stamp.value, timescale: stamp.timescale), image: image)
    }
    func cancel() { reader.cancelReading() }
    deinit { reader.cancelReading() }
}

actor MediaSource {
    let url: URL
    private let generator: AVAssetImageGenerator
    private var requestID: UUID?
    init(url: URL) {
        self.url = url
        generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
        generator.appliesPreferredTrackTransform = true
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = .zero
    }
    func image(at seconds: Double) async throws -> DecodedFrame {
        try Task.checkCancellation()
        let id = UUID(); requestID = id
        generator.cancelAllCGImageGeneration()
        defer { if requestID == id { requestID = nil } }
        return try await withTaskCancellationHandler {
            let result = try await generator.image(at: CMTime(seconds: max(0, seconds), preferredTimescale: 60000))
            try Task.checkCancellation()
            guard requestID == id else { throw CancellationError() }
            return DecodedFrame(stamp: FrameStamp(value: result.actualTime.value, timescale: result.actualTime.timescale), image: result.image)
        } onCancel: { Task { await self.cancelRequest(id) } }
    }
    private func cancelRequest(_ id: UUID) {
        if requestID == id { generator.cancelAllCGImageGeneration() }
    }

    static func timestamps(url: URL) async throws -> [FrameStamp] {
        let asset = AVURLAsset(url: url)
        guard let track = try await asset.loadTracks(withMediaType: .video).first else { throw MediaError.noFrames }
        let reader = try AVAssetReader(asset: asset)
        let output = AVAssetReaderTrackOutput(track: track, outputSettings: nil)
        reader.add(output)
        guard reader.startReading() else { throw reader.error ?? MediaError.decode }
        defer { reader.cancelReading() }
        var stamps: [FrameStamp] = []
        while let sample = output.copyNextSampleBuffer() {
            try Task.checkCancellation()
            let count = CMSampleBufferGetNumSamples(sample)
            for index in 0..<count {
                var info = CMSampleTimingInfo()
                guard CMSampleBufferGetSampleTimingInfo(sample, at: index, timingInfoOut: &info) == noErr else { throw MediaError.decode }
                let time = info.presentationTimeStamp
                if time.isNumeric, time >= .zero { stamps.append(FrameStamp(value: time.value, timescale: time.timescale)) }
            }
        }
        guard reader.status != .failed else { throw reader.error ?? MediaError.decode }
        let sorted = stamps.sorted()
        let unique = sorted.enumerated().filter { $0.offset == 0 || $0.element.seconds > sorted[$0.offset - 1].seconds }.map(\.element)
        guard !unique.isEmpty else { throw MediaError.noFrames }
        return unique
    }
}
