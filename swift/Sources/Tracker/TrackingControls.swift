import SwiftUI

struct TrackingControls: View {
    @ObservedObject var session: VideoSession
    @State private var showSettings = false
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                Label("Autotracker", systemImage: "scope").font(.callout.weight(.semibold))
                Text(session.jobState).font(.callout).foregroundStyle(.secondary)
                if session.isTracking { ProgressView().controlSize(.small) }
                Spacer()
                Button("Settings…") { showSettings = true }.disabled(session.isTracking)
                Button("Start Tracking", action: { if let id = session.selectedTrackID { _ = try? session.beginTracking(trackID: id) } })
                    .disabled(session.seed == nil || !session.canMark)
                Button("Stop", action: session.cancelTracking).disabled(!session.isTracking && session.review == nil)
            }
            if let error = session.trackingError {
                Text(error).font(.callout).foregroundStyle(.primary).textSelection(.enabled)
            } else if let pending = session.review {
                Text(pending.evidence.reason ?? "Uncertain match").font(.callout.weight(.medium))
                HStack {
                    Text("Score \(pending.evidence.score, specifier: "%.4f") · separation \(pending.evidence.separation, specifier: "%.4f") · not a probability")
                        .font(.caption).monospacedDigit()
                    Spacer()
                    Button("Accept & Continue", action: session.acceptCandidate).disabled(pending.evidence.position == nil || session.isSeeking)
                    Button("Skip Frame", action: session.skipCandidate).disabled(session.isSeeking)
                }
                Text("The outlined candidate is unrecorded. To correct it, mark the displayed frame and start again.")
                    .font(.caption).foregroundStyle(.secondary)
            } else if !session.runSummary.isEmpty {
                Text(session.runSummary).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            } else if !session.isTracking {
                Text("Mark a target on the current frame. Dashed boxes show template and local search regions.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.padding(.horizontal, 14).padding(.vertical, 10)
            .sheet(isPresented: $showSettings) { TrackingSettingsView(session: session) }
    }
}

struct TrackingSettingsView: View {
    @ObservedObject var session: VideoSession
    @Environment(\.dismiss) private var dismiss
    @State private var settings = TrackingSettings()
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Tracking settings").font(.title2.weight(.semibold))
            Picker("Matching method", selection: $settings.matchingMethod) {
                ForEach(MatchingMethod.allCases) { Text($0.title).tag($0) }
            }
            Stepper("Template radius: \(settings.templateRadius) px", value: $settings.templateRadius, in: 3...48)
            Stepper("Search radius: \(settings.searchRadius) px", value: $settings.searchRadius, in: 4...128)
            Text("The template must include visible detail. Search stays local to the last accepted position; a lost target pauses for review.")
                .font(.callout).foregroundStyle(.secondary)
            HStack { Text("Minimum score"); TextField("Minimum score", value: $settings.minimumScore, format: .number) }
            HStack { Text("Candidate separation"); TextField("Candidate separation", value: $settings.minimumSeparation, format: .number) }
            Text("Scores and separation depend on the matching method; thresholds are not interchangeable or confidence probabilities. Templates remain fixed. Settings apply to the next run.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Apply") { try? session.configure(settings); dismiss() }.keyboardShortcut(.defaultAction).disabled(!settings.isValid)
            }
        }.padding(24).frame(width: 440).onAppear { settings = session.settings }
    }
}
