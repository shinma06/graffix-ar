import SwiftUI

/// Bottom control surface: color palette + action buttons.
struct ControlBar: View {
    let viewModel: GraffixViewModel
    let onShutter: () -> Void
    let onSave: () -> Void

    var body: some View {
        VStack(spacing: 10) {
            modePicker
            if viewModel.sprayMode == .real {
                capRow
            } else {
                presetRow
            }
            paletteRow
            actionRow
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 20))
        .padding(.horizontal, 10)
        .padding(.bottom, 6)
    }

    // MARK: - Spray mode (カジュアル / リアル)

    private var modePicker: some View {
        Picker("モード", selection: Binding(
            get: { viewModel.sprayMode },
            set: { viewModel.sprayMode = $0 }
        )) {
            Text("カジュアル").tag(SprayModeF.casual)
            Text("リアル").tag(SprayModeF.real)
        }
        .pickerStyle(.segmented)
    }

    // MARK: - Casual presets (書き方)

    private var presetRow: some View {
        HStack(spacing: 8) {
            Text("書き方")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(viewModel.presets, id: \.self) { preset in
                chip(title: preset.label,
                     selected: viewModel.preset == preset) {
                    viewModel.preset = preset
                }
            }
            Spacer()
        }
    }

    // MARK: - Real caps (ノズル) + 缶振り meter

    private var capRow: some View {
        HStack(spacing: 8) {
            Text("ノズル")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(viewModel.caps, id: \.self) { cap in
                chip(title: cap.label,
                     selected: viewModel.cap == cap) {
                    viewModel.cap = cap
                }
            }
            Spacer()
            shakeIndicator
        }
    }

    /// 攪拌レベル — 低いと出がかすれる（リアルモードのみ意味を持つ）。
    private var shakeIndicator: some View {
        let level = viewModel.shakeLevel
        let low = level < 0.3
        return Label(low ? "缶を振れ" : String(format: "攪拌 %.0f%%", level * 100),
                     systemImage: low ? "exclamationmark.triangle" : "waveform")
            .font(.caption)
            .foregroundStyle(low ? .yellow : .secondary)
    }

    private func chip(title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(title, action: action)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(selected ? Color.orange : Color.white.opacity(0.12), in: Capsule())
            .foregroundStyle(selected ? .white : .primary)
    }

    // MARK: - Palette

    private var paletteRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 10) {
                ForEach(viewModel.sprayColors, id: \.id) { entry in
                    let selected = viewModel.color == entry.id
                    Button {
                        viewModel.color = entry.id
                    } label: {
                        Circle()
                            .fill(color(for: entry.rgba))
                            .frame(width: 30, height: 30)
                            .overlay {
                                Circle().stroke(.white, lineWidth: selected ? 3 : 0.5)
                            }
                            .scaleEffect(selected ? 1.15 : 1.0)
                    }
                    .accessibilityLabel(entry.id.label)
                }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 4)
        }
    }

    // MARK: - Actions

    private var actionRow: some View {
        HStack(spacing: 14) {
            // ロック / 解除
            Button {
                if viewModel.isLocked { viewModel.unlock() } else { viewModel.lockAimed() }
            } label: {
                Label(viewModel.isLocked ? "解除" : "ロック",
                      systemImage: viewModel.isLocked ? "lock.open" : "lock")
            }
            .buttonStyle(.bordered)
            .disabled(!viewModel.isLocked && !viewModel.canLock)

            // スプレー（押している間だけ噴射 — 実際のスプレー缶と同じ）
            SprayButton(spraying: Binding(
                get: { viewModel.spraying },
                set: { viewModel.spraying = $0 }
            ))
            .allowsHitTesting(viewModel.isLocked)
            .opacity(viewModel.isLocked ? 1 : 0.4)

            Spacer()

            // 取り消し / やり直し / クリア / 撮影
            Button { viewModel.undo() } label: {
                Image(systemName: "arrow.uturn.backward")
            }
            .disabled(!viewModel.canUndo)

            Button { viewModel.redo() } label: {
                Image(systemName: "arrow.uturn.forward")
            }
            .disabled(!viewModel.canRedo)

            Button(role: .destructive) { viewModel.clearCanvas() } label: {
                Image(systemName: "trash")
            }
            .disabled(!viewModel.canUndo && !viewModel.strokeActive)

            Button { onShutter() } label: {
                Image(systemName: "camera.fill")
            }

            // 作品として保存（キャンバス + 現在地 → マップのピンへ）
            Button { onSave() } label: {
                Image(systemName: "square.and.arrow.down")
            }
            .disabled(!viewModel.canUndo && !viewModel.strokeActive)
        }
        .buttonStyle(.bordered)
        .font(.title3)
    }

    private func color(for c: ColorF) -> Color {
        Color(.sRGB,
              red: Double(c.r) / 255,
              green: Double(c.g) / 255,
              blue: Double(c.b) / 255,
              opacity: Double(c.a) / 255)
    }
}

/// Press-and-hold spray trigger — 押している間だけ `spraying` が立つ。
private struct SprayButton: View {
    @Binding var spraying: Bool

    var body: some View {
        ZStack {
            Circle()
                .fill(spraying ? Color.orange : Color.orange.opacity(0.75))
                .frame(width: 68, height: 68)
                .shadow(radius: spraying ? 2 : 6)
            Text("スプレー")
                .font(.subheadline.weight(.bold))
                .foregroundStyle(.white)
        }
        .scaleEffect(spraying ? 0.94 : 1.0)
        .animation(.easeOut(duration: 0.1), value: spraying)
        .contentShape(Circle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in
                    if !spraying { spraying = true }
                }
                .onEnded { _ in spraying = false }
        )
    }
}
