import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers
import TrackerAutomation

enum FramePreview {
    static func render(_ frame: DecodedFrame, analysis: AnalysisState, revision: Int, crop: [String:JSONValue]?, maxEdge: Int, overlays: Bool) throws -> JSONValue {
        guard (1...1600).contains(maxEdge) else { throw AutomationError("invalid_argument", "max_edge must be 1...1600.") }
        let image = frame.image
        var rect = CGRect(x:0,y:0,width:image.width,height:image.height)
        if let crop {
            guard Set(crop.keys) == Set(["x","y","width","height"]) else { throw AutomationError("invalid_crop", "Supply x, y, width and height only.") }
            let x = try crop.requiredInt("x"), y = try crop.requiredInt("y"), w = try crop.requiredInt("width"), h = try crop.requiredInt("height")
            guard w > 0, h > 0, x < image.width, y < image.height, w <= image.width-x, h <= image.height-y else { throw AutomationError("invalid_crop", "The crop must lie inside source pixels.") }
            rect = CGRect(x:x,y:y,width:w,height:h)
        }
        guard let cropped = image.cropping(to:rect) else { throw MediaError.decode }
        let scale = min(1,Double(maxEdge)/max(rect.width,rect.height))
        let width = max(1,Int((rect.width*scale).rounded())), height = max(1,Int((rect.height*scale).rounded()))
        let sx = Double(width)/rect.width, sy = Double(height)/rect.height
        guard let context = CGContext(data:nil,width:width,height:height,bitsPerComponent:8,bytesPerRow:width*4,space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue) else { throw MediaError.decode }
        context.interpolationQuality = .high
        context.draw(cropped,in:CGRect(x:0,y:0,width:width,height:height))
        if overlays {
            func point(_ p: CGPoint) -> CGPoint { CGPoint(x:(p.x-rect.minX)*sx,y:Double(height)-(p.y-rect.minY)*sy) }
            context.setLineWidth(2); context.setStrokeColor(CGColor(red:0,green:0.7,blue:1,alpha:1))
            for track in analysis.tracks {
                for row in track.points where abs(row.time-frame.stamp.seconds) < 0.000001 {
                    let p = point(row.pixel)
                    context.move(to:CGPoint(x:p.x-5,y:p.y)); context.addLine(to:CGPoint(x:p.x+5,y:p.y))
                    context.move(to:CGPoint(x:p.x,y:p.y-5)); context.addLine(to:CGPoint(x:p.x,y:p.y+5)); context.strokePath()
                }
            }
            if let calibration = analysis.calibration {
                context.setStrokeColor(CGColor(red:1,green:0.7,blue:0,alpha:1))
                context.move(to:point(calibration.first)); context.addLine(to:point(calibration.second)); context.strokePath()
            }
        }
        guard let rendered = context.makeImage() else { throw MediaError.decode }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { throw MediaError.decode }
        CGImageDestinationAddImage(destination,rendered,nil)
        guard CGImageDestinationFinalize(destination) else { throw MediaError.decode }
        return .object(["image_base64":.string((data as Data).base64EncodedString()),"mime_type":.string("image/png"),"time":.number(frame.stamp.seconds),"frame":try .from(frame.stamp),"revision":.number(Double(revision)),"source_width":.number(Double(image.width)),"source_height":.number(Double(image.height)),"image_width":.number(Double(width)),"image_height":.number(Double(height)),"source_origin":.string("top-left"),"orientation_corrected":.bool(true),"overlays":.bool(overlays),"mapping":.object(["crop_x":.number(rect.minX),"crop_y":.number(rect.minY),"crop_width":.number(rect.width),"crop_height":.number(rect.height),"scale_x":.number(sx),"scale_y":.number(sy),"to_source":.string("source_x = image_x / scale_x + crop_x; source_y = image_y / scale_y + crop_y")])])
    }
}
