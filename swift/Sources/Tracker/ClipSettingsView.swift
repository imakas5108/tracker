import SwiftUI

struct ClipSettingsView: View {
    @ObservedObject var session: VideoSession
    @Environment(\.dismiss) private var dismiss
    @State private var startText: String
    @State private var endText: String
    @State private var stepText: String
    @State private var inspectText: String
    @State private var preview: DecodedFrame?
    @State private var previewIndex: Int?
    @State private var previewTask: Task<Void,Never>?
    @State private var previewGeneration = UUID()
    @State private var loadingPreview = false
    @State private var message: String?

    init(session: VideoSession) {
        self.session = session
        _startText = State(initialValue:String(session.clip?.startFrame ?? 0))
        _endText = State(initialValue:String(session.clip?.endFrame ?? 0))
        _stepText = State(initialValue:String(session.clip?.stepSize ?? 1))
        _inspectText = State(initialValue:String(session.currentSourceFrame))
    }
    private var proposal: VideoClip? {
        guard let start = Int(startText), let end = Int(endText), let step = Int(stepText) else { return nil }
        return try? VideoClip.make(start:start,end:end,step:step,source:session.frameTimes)
    }
    private var inspectionIndex: Int? {
        guard let index = Int(inspectText), session.frameTimes.indices.contains(index) else { return nil }
        return index
    }
    var body: some View {
        VStack(alignment:.leading,spacing:14) {
            Text("Clip settings").font(.title2.weight(.semibold))
            Text("Source frames are numbered from 0. The source video and other observations are retained.")
                .font(.callout).foregroundStyle(.secondary)
            Grid(alignment:.leading,verticalSpacing:10) {
                GridRow { Text("Start frame"); TextField("Start frame",text:$startText) }
                GridRow { Text("End bound (inclusive)"); TextField("End frame",text:$endText) }
                GridRow { Text("Step size"); TextField("Step size",text:$stepText) }
            }.textFieldStyle(.roundedBorder)
            if let clip = proposal {
                Text("\(clip.sampleCount) samples · last sampled frame \(clip.lastSampleFrame) · source \(clip.startStamp.seconds, specifier:"%.6f")–\(clip.lastSampleStamp.seconds, specifier:"%.6f") s")
                    .font(.caption).monospacedDigit()
            } else {
                Text("Choose 0 ≤ start ≤ end < \(session.frameTimes.count), with step size ≥ 1.")
                    .font(.caption).foregroundStyle(.red)
            }
            Divider()
            HStack {
                TextField("Inspect source frame",text:$inspectText).textFieldStyle(.roundedBorder)
                    .accessibilityLabel("Inspect source frame")
                Button("Preview") { inspectFrame() }.disabled(inspectionIndex == nil)
            }
            ZStack {
                Color(nsColor:.windowBackgroundColor)
                if let preview {
                    Image(decorative:preview.image,scale:1).resizable().aspectRatio(contentMode:.fit)
                } else { Text("Preview any source frame to choose boundaries.").font(.caption).foregroundStyle(.secondary) }
                if loadingPreview { ProgressView().controlSize(.small) }
            }.frame(height:170).accessibilityLabel("Source frame preview")
            if let index = previewIndex, let preview {
                HStack {
                    Text("Frame \(index) · \(preview.stamp.seconds, specifier:"%.6f") s").font(.caption).monospacedDigit()
                    Spacer()
                    Button("Set Start") { startText = String(index) }.disabled(loadingPreview)
                    Button("Set End") { endText = String(index) }.disabled(loadingPreview)
                }
            }
            if let message { Text(message).font(.callout).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Button("Reset to Full Video") {
                    do { try session.resetClip(); dismiss() } catch { message = error.localizedDescription }
                }
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Apply") {
                    guard let proposal else { return }
                    do { try session.setClip(start:proposal.startFrame,end:proposal.endFrame,step:proposal.stepSize); dismiss() }
                    catch { message = error.localizedDescription }
                }.keyboardShortcut(.defaultAction).disabled(proposal == nil)
            }
        }.padding(20).frame(width:480)
            .onAppear { inspectFrame() }
            .onDisappear { previewGeneration = UUID(); previewTask?.cancel() }
    }
    private func inspectFrame() {
        guard let index = inspectionIndex else { return }
        previewTask?.cancel(); previewGeneration = UUID()
        let generation = previewGeneration, stamp = session.frameTimes[index]
        let reader = MediaSource(url:session.url)
        loadingPreview = true; message = nil
        previewTask = Task {
            do {
                let frame = try await reader.image(at:stamp)
                try Task.checkCancellation()
                guard generation == previewGeneration else { return }
                guard abs(frame.stamp.seconds-stamp.seconds) < 0.000001 else { throw MediaError.decode }
                preview = frame; previewIndex = index; loadingPreview = false
            } catch {
                guard generation == previewGeneration else { return }
                loadingPreview = false
                if !(error is CancellationError) { message = error.localizedDescription }
            }
        }
    }
}
