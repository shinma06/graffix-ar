import SwiftUI
import UIKit

struct ContentView: View {
    @State private var viewModel = GraffixViewModel(engine: Engine())
    @State private var shareItem: ShareItem?

    var body: some View {
        ZStack {
            ARViewContainer(
                engine: viewModel.engine,
                sprayInput: viewModel.sprayInput,
                viewModel: viewModel
            )
            .ignoresSafeArea()

            // Center reticle = the spray nozzle.
            ReticleView(state: reticleState)

            VStack(spacing: 0) {
                statusBar
                Spacer()
                ControlBar(viewModel: viewModel, onShutter: takePhoto)
            }
        }
        .preferredColorScheme(.dark)
        .onAppear { viewModel.startShakeDetection() }
        .onDisappear { viewModel.stopShakeDetection() }
        .sheet(item: $shareItem) { item in
            ShareSheet(items: [item.image])
        }
    }

    // MARK: - Status / reticle

    private var reticleState: ReticleView.State {
        if viewModel.isLocked { return .locked }
        if viewModel.canLock { return .aimed }
        return .searching
    }

    private var statusBar: some View {
        HStack(spacing: 8) {
            Text(viewModel.statusText)
                .font(.subheadline.weight(.medium))
            Spacer()
            if let distance = viewModel.distanceText {
                Label(distance, systemImage: viewModel.lidarFused ? "ruler.fill" : "ruler")
                    .font(.caption.monospacedDigit())
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.ultraThinMaterial, in: Capsule())
        .padding(.horizontal, 16)
        .padding(.top, 8)
    }

    // MARK: - Photo / share

    private func takePhoto() {
        Task {
            guard let image = await viewModel.takeSnapshot() else { return }
            shareItem = ShareItem(image: image)
        }
    }
}

/// `sheet(item:)` payload for the share UI.
private struct ShareItem: Identifiable {
    let id = UUID()
    let image: UIImage
}

/// 画面中央のレティクル（スプレー缶のノズル位置）。
struct ReticleView: View {
    enum State {
        case searching
        case aimed
        case locked
    }

    let state: State

    private var color: Color {
        switch state {
        case .searching: .white.opacity(0.6)
        case .aimed: .cyan
        case .locked: .green
        }
    }

    var body: some View {
        ZStack {
            Circle()
                .stroke(color, lineWidth: 2)
                .frame(width: 26, height: 26)
            Circle()
                .fill(color)
                .frame(width: 4, height: 4)
        }
        .shadow(color: .black.opacity(0.5), radius: 2)
        .animation(.easeInOut(duration: 0.15), value: state)
    }
}
