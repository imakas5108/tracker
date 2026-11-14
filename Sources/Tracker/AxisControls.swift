import SwiftUI

struct AxisControls: View {
    @ObservedObject var session: VideoSession
    @State private var x = 0.0
    @State private var y = 0.0
    @State private var angle = 0.0
    var body: some View {
        HStack(spacing:8) {
            Text("Origin (px)").font(.caption)
            field("Origin x",value:$x)
            field("Origin y",value:$y)
            Text("Angle (°)").font(.caption)
            field("Axis angle",value:$angle)
            Button("Apply Axes",action:apply)
            Button("Reset") {
                do { try session.setCoordinates(CoordinateSystem()) }
                catch { session.exportError = error.localizedDescription }
            }
            Spacer(minLength:0)
            Text("Source pixels · angle counterclockwise").font(.caption).foregroundStyle(.secondary)
        }.controlSize(.small).padding(.horizontal,14).padding(.vertical,6)
            .onAppear(perform:sync)
            .onChange(of:session.coordinateSystem) { _,_ in sync() }
            .onChange(of:session.size) { _,_ in sync() }
            .disabled(session.loading || session.error != nil)
    }
    private func field(_ label: String, value: Binding<Double>) -> some View {
        TextField(label,value:value,format:.number.grouping(.never).precision(.fractionLength(0...3)))
            .frame(width:72).textFieldStyle(.roundedBorder).accessibilityLabel(label).onSubmit(apply)
    }
    private func sync() {
        let origin = session.coordinateSystem.origin(in:session.size)
        x = origin.x; y = origin.y; angle = session.coordinateSystem.angleDegrees
    }
    private func apply() {
        do { try session.setCoordinates(CoordinateSystem(origin:CGPoint(x:x,y:y),angleDegrees:angle)) }
        catch { session.exportError = error.localizedDescription }
    }
}
