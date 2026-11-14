import SwiftUI
import AVKit

struct WorkspaceView: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        VStack(spacing: 0) {
            if !workspace.sessions.isEmpty {
                HStack(spacing: 0) {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 0) {
                            ForEach(workspace.sessions) { session in
                                HStack(spacing: 8) {
                                    Button { workspace.select(session.id) } label: {
                                        Label(session.name, systemImage: "film")
                                            .lineLimit(1).frame(maxWidth: 190)
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityAddTraits(workspace.selectedID == session.id ? .isSelected : [])
                                    Button { workspace.close(session.id) } label: {
                                        Image(systemName: "xmark").font(.system(size: 9, weight: .semibold))
                                    }
                                    .buttonStyle(.borderless).help("Close \(session.name)")
                                    .accessibilityLabel("Close \(session.name)")
                                }
                                .padding(.horizontal, 14).frame(height: 36)
                                .background(workspace.selectedID == session.id ? Color(nsColor: .controlBackgroundColor) : .clear)
                                Divider().frame(height: 18)
                            }
                        }
                    }
                    Text("\(workspace.sessions.count) video\(workspace.sessions.count == 1 ? "" : "s")")
                        .font(.caption).foregroundStyle(.secondary).padding(.horizontal, 14)
                }
                .background(Color(nsColor: .windowBackgroundColor))
                Divider()
            }
            if let session = workspace.selected {
                SessionView(session: session).id(session.id)
            } else {
                EmptyWorkspace(open: workspace.openVideos)
            }
            Divider()
            AgentControlView(controller: workspace.automation)
        }
        .alert("Project", isPresented: Binding(get: { workspace.message != nil }, set: { if !$0 { workspace.message = nil } })) {
            Button("OK") { workspace.message = nil }
        } message: { Text(workspace.message ?? "") }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button(action: workspace.openVideos) { Label("Open Video", systemImage: "folder") }
                    .help("Open one or more videos (⌘O)")
            }
            ToolbarItem(placement: .principal) {
                Text("Tracker").font(.headline)
            }
            ToolbarItem {
                Button(action: workspace.chooseProject) { Label("Open Project", systemImage: "doc") }
            }
        }
    }
}

struct AgentControlView: View {
    @ObservedObject var controller: AutomationController
    @State private var showActions = false
    var body: some View {
        HStack(spacing: 12) {
            Text(controller.endpointError != nil ? "Agent control unavailable" : controller.enabled ? "Agent control allowed · \(controller.clients.count) connected" : "Agent control stopped")
                .font(.caption).foregroundStyle(.secondary)
            Button("Recent Agent Actions") { showActions.toggle() }
                .popover(isPresented: $showActions) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Agent activity").font(.headline)
                        if let error = controller.endpointError { Text(error).textSelection(.enabled) }
                        if controller.actions.isEmpty { Text("No agent actions in this app session.").foregroundStyle(.secondary) }
                        ScrollView {
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(Array(controller.actions.enumerated()), id: \.offset) { _, action in Text(action).textSelection(.enabled) }
                            }.frame(maxWidth: .infinity, alignment: .leading)
                        }.frame(maxHeight: 260)
                        Text("Tracking jobs belong to Tracker. Disconnecting or stopping control leaves them available in the workspace.")
                            .font(.caption).foregroundStyle(.secondary)
                    }.padding(16).frame(width: 440)
                }
            Spacer()
            if controller.enabled {
                Button("Stop Agent Control", action: controller.stopControl)
            } else {
                Button("Allow Agent Control", action: controller.allowControl).disabled(controller.endpointError != nil)
            }
        }.buttonStyle(.borderless).padding(.horizontal, 14).padding(.vertical, 7)
    }
}

struct EmptyWorkspace: View {
    var open: () -> Void
    var body: some View {
        HSplitView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Tracks").font(.headline)
                Text("Your point tracks and calibration will appear here.")
                    .font(.callout).foregroundStyle(.secondary)
                Spacer()
            }.padding(18).frame(width: 190, alignment: .topLeading).frame(maxHeight: .infinity)
            VStack(spacing: 0) {
                Spacer()
                VStack(spacing: 16) {
                    Image(systemName: "film").font(.system(size: 36, weight: .light)).foregroundStyle(.secondary)
                    Text("Open a video to begin").font(.title2.weight(.medium))
                    Text("Set a scale, mark positions, and inspect the motion.")
                        .foregroundStyle(.secondary)
                    Button("Open Video…", action: open).keyboardShortcut(.defaultAction)
                    Text("MOV, MP4, and other formats supported by macOS")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Text("Video analysis workspace · Interface prototype")
                    .font(.caption).foregroundStyle(.secondary).padding(16)
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
            VStack(spacing: 0) {
                PanelHeader(title: "Plot", symbol: "chart.xyaxis.line")
                ContentUnavailableView("No track data", systemImage: "chart.xyaxis.line",
                                       description: Text("Mark positions to plot motion over time."))
                Divider()
                PanelHeader(title: "Data", symbol: "tablecells")
                ContentUnavailableView("No measurements", systemImage: "tablecells",
                                       description: Text("Time and coordinates will appear here."))
            }.frame(minWidth: 260, idealWidth: 300, maxWidth: 400, maxHeight: .infinity)
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct SessionView: View {
    @ObservedObject var session: VideoSession
    @State private var showPositionEntry = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 14) {
                HStack(spacing: 6) {
                    ForEach(WorkspaceTool.allCases, id: \.self) { tool in
                        Button { session.selectTool(tool) } label: {
                            Label(tool.rawValue, systemImage: tool.symbol)
                        }
                        .tint(session.tool == tool ? .accentColor : nil)
                        .accessibilityAddTraits(session.tool == tool ? .isSelected : [])
                    }
                }.buttonStyle(.bordered)
                Toggle(isOn: $session.showAxes) { Label("Axes", systemImage: "arrow.up.and.right") }
                    .toggleStyle(.button).help("Show axes and edit their origin and angle")
                Divider().frame(height: 20)
                Button { Task { _ = await session.saveProject() } } label: {
                    Label(session.saving ? "Saving…" : "Save Project", systemImage: "square.and.arrow.down")
                }.disabled(session.saving || session.isTracking)
                Text(session.isDirty ? "Unsaved changes" : "Saved state").font(.caption).foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .disabled(session.loading || session.error != nil)
            Divider()
            if session.showAxes {
                AxisControls(session:session)
                Divider()
            }
            TrackingControls(session: session)
            Divider()
            HSplitView {
                trackList.frame(minWidth: 170, idealWidth: 190, maxWidth: 220)
                VStack(spacing: 0) {
                    HStack {
                        Text(session.selectedTrack?.name ?? "Video").font(.callout.weight(.medium))
                        Spacer()
                        Text("\(Int(session.size.width)) × \(Int(session.size.height)) px")
                            .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    }.padding(.horizontal, 14).frame(height: 36)
                    VideoCanvas(session: session)
                    PlaybackBar(session: session)
                }.frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity).layoutPriority(1)
                VSplitView {
                    PositionPlot(session: session).frame(minHeight: 200)
                    DataPanel(session: session).frame(minHeight: 180)
                }.frame(minWidth: 270, idealWidth: 290, maxWidth: 420)
            }
            Divider()
            HStack {
                Text(session.status).lineLimit(1)
                Spacer()
                Text("\(session.points.count) \(session.points.count == 1 ? "point" : "points") · \(session.unit)").monospacedDigit()
            }.font(.caption).foregroundStyle(.secondary).padding(.horizontal, 14).frame(height: 28)
        }
        .sheet(isPresented: $session.showCalibration) { CalibrationSheet(session: session) }
        .sheet(isPresented: $showPositionEntry) { PositionEntrySheet(session: session) }
        .alert("Analysis", isPresented: Binding(get: { session.exportError != nil }, set: { if !$0 { session.exportError = nil } })) {
            Button("OK") { session.exportError = nil }
        } message: { Text(session.exportError ?? "") }
    }

    private var trackList: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Tracks").font(.headline)
                Spacer()
                Button(action: session.addTrack) { Image(systemName: "plus") }
                    .help("Add a point mass").accessibilityLabel("Add a point mass")
                    .disabled(session.loading || session.error != nil)
            }.padding(14)
            if session.tracks.isEmpty {
                Text("Add a point mass to start marking positions.")
                    .font(.callout).foregroundStyle(.secondary).padding(.horizontal, 14)
            } else {
                List(selection: $session.selectedTrackID) {
                    ForEach(session.tracks) { track in
                        Label(track.name, systemImage: "plus.circle").tag(track.id)
                    }
                }.listStyle(.sidebar)
            }
            Spacer(minLength: 16)
            if session.selectedTrack != nil {
                Button("Enter Position…") { session.pause(); showPositionEntry = true }
                    .padding(14).keyboardShortcut("m", modifiers: [.command, .shift])
            }
            Divider()
            VStack(alignment: .leading, spacing: 8) {
                Label("Scale", systemImage: "ruler").font(.callout.weight(.semibold))
                if let calibration = session.calibration {
                    Text("\(calibration.pixelsPerUnit, specifier: "%.2f") px / \(calibration.unit)")
                        .font(.callout.monospacedDigit())
                    Button("Reset Scale") { session.calibration = nil; session.calibrationPoints = [] }
                } else {
                    Text("Not calibrated").font(.callout)
                    Text("Use Calibrate to select a known distance.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Button("Set Scale…") { session.pause(); session.showCalibration = true }
                    .disabled(session.loading || session.error != nil)
                Text("Axes: \(session.coordinateSystem.angleDegrees, specifier: "%.1f")°\nShow Axes to set the origin.")
                    .font(.caption).foregroundStyle(.secondary).padding(.top, 8)
            }.padding(14)
        }.background(Color(nsColor: .windowBackgroundColor))
    }
}

struct PanelHeader: View {
    var title: String
    var symbol: String
    var body: some View {
        HStack {
            Label(title, systemImage: symbol).font(.callout.weight(.semibold))
            Spacer()
        }.padding(.horizontal, 12).frame(height: 36)
            .background(Color(nsColor: .windowBackgroundColor))
    }
}

struct VideoCanvas: View {
    @ObservedObject var session: VideoSession
    var body: some View {
        GeometryReader { geometry in
            let sourceSize = session.size
            let rect = VideoGeometry.fittedRect(source: sourceSize, container: geometry.size)
            ZStack {
                Color(nsColor: NSColor(white: 0.09, alpha: 1))
                if session.loading {
                    ProgressView("Opening video…").tint(.white).foregroundStyle(.white)
                } else if let error = session.error {
                    VStack(spacing: 12) {
                        Image(systemName: "exclamationmark.triangle").font(.title)
                        Text(error).multilineTextAlignment(.center)
                    }.padding(28).foregroundStyle(.white)
                } else {
                    if !session.playing, let frame = session.displayedFrame {
                        Image(decorative: frame.image, scale: 1).resizable().aspectRatio(contentMode: .fit)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else { NativeVideo(player: session.player) }
                    if session.isSeeking {
                        Text("Loading frame…").padding(8).background(.black).foregroundStyle(.white)
                    }
                    Canvas { context, _ in
                        func project(_ point: CGPoint) -> CGPoint {
                            CGPoint(x: rect.minX + point.x / sourceSize.width * rect.width,
                                    y: rect.minY + point.y / sourceSize.height * rect.height)
                        }
                        if session.showAxes {
                            let origin = project(session.coordinateSystem.origin(in:sourceSize))
                            let angle = session.coordinateSystem.angleDegrees * .pi/180
                            let radius = max(rect.width,rect.height)*2
                            context.clip(to:Path(rect))
                            for (dx,dy,label) in [(cos(angle),-sin(angle),"x"),(-sin(angle),-cos(angle),"y")] {
                                var path = Path()
                                path.move(to:CGPoint(x:origin.x-dx*radius,y:origin.y-dy*radius))
                                path.addLine(to:CGPoint(x:origin.x+dx*radius,y:origin.y+dy*radius))
                                context.stroke(path,with:.color(.black),lineWidth:3)
                                context.stroke(path,with:.color(.white),lineWidth:1)
                                let labelPoint = CGPoint(x:origin.x+dx*45+5,y:origin.y+dy*45-8)
                                context.fill(Path(CGRect(x:labelPoint.x-8,y:labelPoint.y-9,width:16,height:18)),with:.color(.black))
                                context.draw(Text(label).foregroundColor(.white),at:labelPoint)
                            }
                            context.stroke(Path(ellipseIn:CGRect(x:origin.x-6,y:origin.y-6,width:12,height:12)),with:.color(.black),lineWidth:4)
                            context.stroke(Path(ellipseIn:CGRect(x:origin.x-6,y:origin.y-6,width:12,height:12)),with:.color(.white),lineWidth:2)

                        }
                        for point in session.points {
                            let center = project(point.pixel)
                            var cross = Path()
                            cross.move(to: CGPoint(x: center.x - 6, y: center.y)); cross.addLine(to: CGPoint(x: center.x + 6, y: center.y))
                            cross.move(to: CGPoint(x: center.x, y: center.y - 6)); cross.addLine(to: CGPoint(x: center.x, y: center.y + 6))
                            context.stroke(cross, with: .color(.black), lineWidth: 4)
                            context.stroke(cross, with: .color(.white), lineWidth: 2)
                        }
                        if let review = session.review, let point = review.evidence.position {
                            let center = project(point)
                            context.stroke(Path(ellipseIn: CGRect(x: center.x-10, y: center.y-10, width: 20, height: 20)), with: .color(.black), lineWidth: 5)
                            context.stroke(Path(ellipseIn: CGRect(x: center.x-10, y: center.y-10, width: 20, height: 20)), with: .color(.yellow), lineWidth: 2)
                        }
                        if let seed = session.seed {
                            let center = project(seed.pixel)
                            for radius in [session.settings.templateRadius, session.settings.searchRadius] {
                                let rx = CGFloat(radius) / sourceSize.width * rect.width
                                let ry = CGFloat(radius) / sourceSize.height * rect.height
                                let box = Path(CGRect(x: center.x-rx, y: center.y-ry, width: rx*2, height: ry*2))
                                context.stroke(box, with: .color(.black), lineWidth: 3)
                                context.stroke(box, with: .color(.white), style: StrokeStyle(lineWidth: 1, dash: [4,3]))
                            }
                        }
                        let ends = session.calibrationPoints.isEmpty ? [session.calibration?.first, session.calibration?.second].compactMap { $0 } : session.calibrationPoints
                        if let first = ends.first {
                            var line = Path()
                            line.move(to: project(first))
                            if ends.count > 1 { line.addLine(to: project(ends[1])) }
                            context.stroke(line, with: .color(.black), lineWidth: 5)
                            context.stroke(line, with: .color(.white), lineWidth: 2)
                            for point in ends {
                                let center = project(point)
                                context.fill(Path(ellipseIn: CGRect(x: center.x - 4, y: center.y - 4, width: 8, height: 8)), with: .color(.white))
                            }
                        }
                    }.allowsHitTesting(false)
                    if session.showAxes && session.tool == .navigate {
                        Color.clear.contentShape(Rectangle()).gesture(DragGesture(minimumDistance:3).onEnded { drag in
                            let origin = session.coordinateSystem.origin(in:sourceSize)
                            let center = CGPoint(x:rect.minX+origin.x/sourceSize.width*rect.width,y:rect.minY+origin.y/sourceSize.height*rect.height)
                            guard hypot(drag.startLocation.x-center.x,drag.startLocation.y-center.y)<18,
                                  let point = VideoGeometry.sourcePoint(drag.location,source:sourceSize,rect:rect) else { return }
                            do { try session.setCoordinates(CoordinateSystem(origin:point,angleDegrees:session.coordinateSystem.angleDegrees)) }
                            catch { session.exportError = error.localizedDescription }
                        }).accessibilityLabel("Drag coordinate origin; use Origin fields for keyboard entry")
                    }
                    if session.tool != .navigate && session.canMark {
                        Color.clear.contentShape(Rectangle())
                            .onTapGesture { location in
                                if let point = VideoGeometry.sourcePoint(location, source: session.size, rect: rect) { session.mark(point) }
                            }
                            .accessibilityLabel(session.tool == .mark ? "Video marking area" : "Calibration endpoint area")
                    }
                }
            }
        }.clipped()
    }
}

struct NativeVideo: NSViewRepresentable {
    var player: AVPlayer
    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .none
        view.videoGravity = .resizeAspect
        view.player = player
        return view
    }
    func updateNSView(_ view: AVPlayerView, context: Context) { view.player = player }
}

struct PlaybackBar: View {
    @ObservedObject var session: VideoSession
    var body: some View {
        VStack(spacing: 10) {
            Slider(value: Binding(get: { session.time }, set: { session.pause(); session.seek($0) }),
                   in: 0...max(session.duration, 0.01))
                .accessibilityLabel("Video time")
            HStack(spacing: 16) {
                Button { session.seek(0); session.pause() } label: { Image(systemName: "backward.end.fill") }
                    .help("Go to start").accessibilityLabel("Go to start")
                Button { session.step(-1) } label: { Image(systemName: "backward.frame") }
                    .help("Previous frame").accessibilityLabel("Previous frame")
                Button(action: session.togglePlayback) {
                    Image(systemName: session.playing ? "pause.fill" : "play.fill").frame(width: 16)
                }.help("Play or pause (Space)").accessibilityLabel(session.playing ? "Pause" : "Play")
                    .keyboardShortcut(.space, modifiers: [])
                Button { session.step(1) } label: { Image(systemName: "forward.frame") }
                    .help("Next frame").accessibilityLabel("Next frame")
                Spacer()
                Text("\(session.time, specifier: "%.3f") / \(session.duration, specifier: "%.3f") s")
                    .font(.system(.callout, design: .monospaced)).monospacedDigit()
            }.buttonStyle(.borderless)
        }.padding(14).background(Color(nsColor: .windowBackgroundColor))
            .disabled(session.loading || session.error != nil)
    }
}
