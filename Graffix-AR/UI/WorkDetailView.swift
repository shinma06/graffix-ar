import SwiftUI
import UIKit

/// 作品詳細 — シーンシミュレーションと描画タイムラプス。
/// 合成・照明・視距離・再生タイミングの計算はすべて Rust コア
/// （`SavedWork`）が行い、ここは操作 UI と画像の表示だけを受け持つ。
struct WorkDetailView: View {
    let record: LocalWorkRecord
    let store: LocalWorkStore
    let onShowOnMap: (LocalWorkRecord) -> Void

    private enum Mode: Hashable {
        case scene
        case timelapse
    }

    @State private var work: SavedWork?
    @State private var info: SavedWorkInfoF?
    @State private var loadError: String?
    @State private var mode = Mode.scene

    var body: some View {
        Group {
            if let work, let info {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        Picker("表示", selection: $mode) {
                            Text("シーン").tag(Mode.scene)
                            Text("タイムラプス").tag(Mode.timelapse)
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("workDetail.mode")

                        switch mode {
                        case .scene:
                            ScenePreviewSection(work: work, info: info)
                        case .timelapse:
                            TimelapsePlayerSection(work: work, info: info)
                        }
                    }
                    .padding()
                }
            } else if let loadError {
                ContentUnavailableView(
                    "作品を開けません",
                    systemImage: "exclamationmark.triangle",
                    description: Text(loadError)
                )
            } else {
                ProgressView()
            }
        }
        .navigationTitle(record.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if record.coordinate != nil {
                Button {
                    onShowOnMap(record)
                } label: {
                    Label("マップで見る", systemImage: "mappin.and.ellipse")
                }
            }
        }
        .task(id: record.id) { await load() }
    }

    private func load() async {
        do {
            let bytes = try store.canvasBytes(for: record)
            let loaded = try await Task.detached(priority: .userInitiated) {
                try SavedWork(bytes: bytes)
            }.value
            work = loaded
            info = loaded.info()
        } catch {
            loadError = error.localizedDescription
        }
    }
}

// MARK: - Scene simulation

/// シーンシミュレーション: 照明・壁の素材/色・視距離を選んでプレビュー。
private struct ScenePreviewSection: View {
    let work: SavedWork
    let info: SavedWorkInfoF

    private static let options = sceneOptions()
    private static let imageSize = (width: UInt32(800), height: UInt32(600))

    @State private var lighting = SceneLightingF.day
    @State private var material = WallMaterialF.concrete
    @State private var wallColorIndex: UInt8 = 0
    @State private var distance = ScenePreviewSection.options.defaultViewingDistanceM
    @State private var image: UIImage?
    @State private var viewWidthM: Float = 0

    private struct RenderKey: Hashable {
        var scene: SceneConfigF
        var distance: Float
    }

    private var scene: SceneConfigF {
        SceneConfigF(lighting: lighting, material: material, wallColorIndex: wallColorIndex)
    }

    var body: some View {
        let options = Self.options
        VStack(alignment: .leading, spacing: 14) {
            PreviewImage(image: image, label: "シーンプレビュー")
                .accessibilityIdentifier("workDetail.scene.preview")

            Text(String(format: "%.1f m 離れて見たとき — 視野の幅 %.1f m（水平画角 %.0f°）／作品 %.2f × %.2f m",
                        distance, viewWidthM, options.cameraHfovDeg, info.widthM, info.heightM))
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("workDetail.scene.caption")

            Picker("照明", selection: $lighting) {
                ForEach(options.lightings, id: \.self) { option in
                    Text(option.name).tag(option.lighting)
                }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("workDetail.scene.lighting")

            Picker("壁の素材", selection: $material) {
                ForEach(options.materials, id: \.self) { option in
                    Text(option.name).tag(option.material)
                }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("workDetail.scene.material")

            VStack(alignment: .leading, spacing: 6) {
                Text("壁の色").font(.subheadline)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(Array(options.wallColors.enumerated()), id: \.offset) { index, entry in
                            wallColorSwatch(index: UInt8(index), entry: entry)
                        }
                    }
                    .padding(.vertical, 2)
                }
            }

            VStack(alignment: .leading, spacing: 4) {
                Text(String(format: "視距離 %.1f m", distance)).font(.subheadline)
                Slider(value: $distance,
                       in: options.minViewingDistanceM...options.maxViewingDistanceM,
                       step: 0.5) {
                    Text("視距離")
                } minimumValueLabel: {
                    Text(String(format: "%.1f", options.minViewingDistanceM)).font(.caption2)
                } maximumValueLabel: {
                    Text(String(format: "%.0f m", options.maxViewingDistanceM)).font(.caption2)
                }
                .accessibilityValue(String(format: "%.1f メートル", distance))
                .accessibilityIdentifier("workDetail.scene.distance")
            }
        }
        .task(id: RenderKey(scene: scene, distance: distance)) {
            // Debounce slider drags a little; a newer key cancels this one.
            try? await Task.sleep(for: .milliseconds(40))
            guard !Task.isCancelled else { return }
            let work = work, scene = scene, meters = distance, size = Self.imageSize
            let raster = await Task.detached(priority: .userInitiated) {
                work.renderScene(scene: scene,
                                 framing: .viewingDistance(meters: meters),
                                 width: size.width, height: size.height)
            }.value
            guard !Task.isCancelled else { return }
            image = UIImage(raster: raster)
            viewWidthM = raster.viewWidthM
        }
    }

    private func wallColorSwatch(index: UInt8, entry: PaletteEntryF) -> some View {
        let selected = index == wallColorIndex
        return Button {
            wallColorIndex = index
        } label: {
            VStack(spacing: 4) {
                Circle()
                    .fill(swiftUIColor(entry.color))
                    .overlay {
                        if index == 0 {
                            Image(systemName: "square.grid.3x3.fill")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .overlay(Circle().stroke(selected ? Color.accentColor : .secondary.opacity(0.4),
                                             lineWidth: selected ? 3 : 1))
                    .frame(width: 34, height: 34)
                Text(entry.name).font(.caption2).foregroundStyle(.primary)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(entry.name)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("workDetail.scene.wallColor.\(index)")
    }
}

// MARK: - Timelapse

/// 描画タイムラプス: 描いた順・記録時刻どおりにストロークを再生する。
private struct TimelapsePlayerSection: View {
    let work: SavedWork
    let info: SavedWorkInfoF

    /// タイムラプスは作品が見やすい明るい白壁で再生する。
    private static let scene = SceneConfigF(lighting: .overcast, material: .plaster, wallColorIndex: 0)
    private static let imageSize = (width: UInt32(800), height: UInt32(600))
    private static let speeds: [Double] = [1, 2, 4, 8]

    @State private var timeMs: Double = .infinity
    @State private var playing = false
    @State private var speed: Double = 2
    @State private var image: UIImage?
    @State private var progress: TimelapseProgressF?
    @State private var exporter = TimelapseExportModel()

    private var duration: Double { Double(info.timelapseDurationMs) }

    private var exportRequest: TimelapseExportRequestF {
        defaultTimelapseExportRequest(speed: Float(speed))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            PreviewImage(image: image, label: "タイムラプス")
                .accessibilityIdentifier("workDetail.timelapse.preview")

            HStack {
                Button {
                    playing.toggle()
                } label: {
                    Image(systemName: playing ? "pause.fill" : "play.fill")
                        .font(.title2)
                        .frame(width: 44, height: 44)
                }
                .buttonStyle(.borderedProminent)
                .disabled(info.strokeCount == 0)
                .accessibilityLabel(playing ? "一時停止" : "再生")
                .accessibilityIdentifier("workDetail.timelapse.playPause")

                Slider(value: Binding(get: { min(timeMs, duration) },
                                      set: { timeMs = $0; playing = false }),
                       in: 0...max(duration, 1)) {
                    Text("再生位置")
                }
                .disabled(info.strokeCount == 0)
                .accessibilityIdentifier("workDetail.timelapse.scrubber")
            }

            Picker("速度", selection: $speed) {
                ForEach(Self.speeds, id: \.self) { s in
                    Text("×\(Int(s))").tag(s)
                }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("workDetail.timelapse.speed")

            if let progress {
                Text(statusText(progress))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("workDetail.timelapse.status")
            }
            if !info.hasRecordedTiming && info.strokeCount > 0 {
                Label("記録時刻のない旧形式の作品です。描いた順に一定の速さで再生します。",
                      systemImage: "clock.badge.questionmark")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("workDetail.timelapse.legacyNote")
            }

            exportSection
        }
        .sheet(item: $exporter.exported) { video in
            ShareSheet(items: [video.url])
        }
        .onDisappear { exporter.cancel() }
        .task(id: playing) {
            guard playing else { return }
            if timeMs >= duration { timeMs = 0 }
            var last = ContinuousClock.now
            while !Task.isCancelled {
                await render(at: timeMs)
                try? await Task.sleep(for: .milliseconds(16))
                let now = ContinuousClock.now
                let elapsed = now - last
                last = now
                let ms = Double(elapsed.components.seconds) * 1000
                    + Double(elapsed.components.attoseconds) / 1e15
                timeMs = min(timeMs + ms * speed, duration)
                if timeMs >= duration {
                    await render(at: timeMs)
                    playing = false
                    break
                }
            }
        }
        .task(id: playing ? -1 : min(timeMs, duration)) {
            // Paused / scrubbing: show the frame at the slider position.
            guard !playing else { return }
            await render(at: min(timeMs, duration))
        }
    }

    private func render(at ms: Double) async {
        let tMs = UInt32(max(0, min(ms, Double(UInt32.max))))
        let work = work, scene = Self.scene, size = Self.imageSize
        let raster = await Task.detached(priority: .userInitiated) {
            work.renderTimelapseFrame(tMs: tMs, scene: scene, width: size.width, height: size.height)
        }.value
        image = UIImage(raster: raster)
        progress = work.timelapseProgress(tMs: tMs)
    }

    /// 動画書き出し: 長さ・サイズ・速度はコアの計画（`timelapseExportPlan`）をそのまま表示する。
    private var exportSection: some View {
        let plan = work.timelapseExportPlan(request: exportRequest)
        return VStack(alignment: .leading, spacing: 8) {
            if exporter.isExporting {
                ProgressView(value: exporter.progress) {
                    Text("動画を書き出し中…")
                } currentValueLabel: {
                    Text("\(Int(exporter.progress * 100))%")
                }
                Button("キャンセル", role: .cancel) {
                    exporter.cancel()
                }
                .buttonStyle(.bordered)
            } else {
                Button {
                    playing = false
                    exporter.start(work.timelapseExport(request: exportRequest, scene: Self.scene))
                } label: {
                    Label("動画を書き出して共有", systemImage: "square.and.arrow.up")
                }
                .buttonStyle(.bordered)
                .disabled(info.strokeCount == 0)
            }

            Text(String(format: "%d × %d ・ %d fps ・ 約 %.1f 秒（%@、最後に完成画を表示）",
                        plan.width, plan.height, plan.fps,
                        Double(plan.videoDurationMs) / 1000, speedText(plan.speed)))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            if plan.spedUp {
                Label(String(format: "長い作品のため、%d 秒に収まるよう %@ で書き出します。",
                             plan.maxVideoMs / 1000, speedText(plan.speed)),
                      systemImage: "hare")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let message = exporter.errorMessage {
                Label(message, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
    }

    private func speedText(_ speed: Float) -> String {
        speed == speed.rounded() ? "×\(Int(speed))" : String(format: "×%.1f", speed)
    }

    private func statusText(_ p: TimelapseProgressF) -> String {
        let shown = p.activeStroke.map { $0 + 1 } ?? p.completedStrokes
        let seconds = min(timeMs, duration) / 1000
        return String(format: "ストローク %d / %d ・ %.1f / %.1f 秒",
                      shown, info.strokeCount, seconds, duration / 1000)
    }
}

// MARK: - Shared

private struct PreviewImage: View {
    let image: UIImage?
    let label: String

    var body: some View {
        ZStack {
            Rectangle().fill(.quaternary)
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .interpolation(.medium)
                    .scaledToFit()
            } else {
                ProgressView()
            }
        }
        .aspectRatio(4.0 / 3.0, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .accessibilityElement()
        .accessibilityLabel(label)
        .accessibilityAddTraits(.isImage)
    }
}

private func swiftUIColor(_ c: ColorF) -> Color {
    Color(.sRGB,
          red: Double(c.r) / 255,
          green: Double(c.g) / 255,
          blue: Double(c.b) / 255,
          opacity: Double(c.a) / 255)
}
