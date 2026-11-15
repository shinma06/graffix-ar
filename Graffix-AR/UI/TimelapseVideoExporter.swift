import Accelerate
import AVFoundation
import CoreVideo
import Foundation
import Observation

/// 書き出し済みのタイムラプス動画（共有シートに渡す）。
struct ExportedTimelapseVideo: Identifiable {
    let id = UUID()
    let url: URL
}

/// タイムラプス動画の書き出し状態（進捗・キャンセル・結果）。
/// フレーム時刻・長さ・サイズ・ビットレートはコアの計画（`TimelapseExport`）
/// どおりで、ここはエンコード処理の起動と UI 向けの状態だけを持つ。
@MainActor
@Observable
final class TimelapseExportModel {
    private(set) var progress: Double = 0
    private(set) var isExporting = false
    var errorMessage: String?
    var exported: ExportedTimelapseVideo?

    @ObservationIgnored private var task: Task<Void, Never>?

    func start(_ job: TimelapseExport) {
        guard task == nil else { return }
        progress = 0
        errorMessage = nil
        isExporting = true
        let report: @MainActor @Sendable (Double) -> Void = { [weak self] value in
            self?.progress = value
        }
        let encoding = Task.detached(priority: .userInitiated) {
            try await TimelapseVideoExporter.export(job, progress: report)
        }
        task = Task { [weak self] in
            let result = await withTaskCancellationHandler {
                await encoding.result
            } onCancel: {
                encoding.cancel()
            }
            guard let self else { return }
            self.task = nil
            self.isExporting = false
            switch result {
            case .success(let url):
                self.exported = ExportedTimelapseVideo(url: url)
            case .failure(let error) where error is CancellationError:
                break
            case .failure(let error):
                self.errorMessage = error.localizedDescription
            }
        }
    }

    func cancel() {
        task?.cancel()
    }
}

/// コアが描いたタイムラプスのフレームを H.264 / MP4 にエンコードするだけ。
enum TimelapseVideoExporter {
    enum ExportError: LocalizedError {
        case cannotStart(String?)
        case pixelBuffer
        case writeFailed(String?)

        var errorDescription: String? {
            switch self {
            case .cannotStart(let reason):
                return "動画の書き出しを開始できませんでした。" + (reason ?? "")
            case .pixelBuffer:
                return "動画のフレームを用意できませんでした。"
            case .writeFailed(let reason):
                return "動画の書き出しに失敗しました。" + (reason ?? "")
            }
        }
    }

    /// 書き出し先（一時ディレクトリ）。前回の書き出しは消してから使う。
    static func prepareOutputURL() throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appending(path: "timelapse-export", directoryHint: .isDirectory)
        try? FileManager.default.removeItem(at: dir)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appending(path: "Graffix-timelapse.mp4", directoryHint: .notDirectory)
    }

    /// `job` の全フレームを順にエンコードし、書き出した MP4 の URL を返す。
    /// キャンセルされると `CancellationError` を投げ、途中のファイルは消す。
    static func export(_ job: TimelapseExport,
                       progress: @escaping @MainActor @Sendable (Double) -> Void) async throws -> URL {
        let plan = job.plan()
        let url = try prepareOutputURL()
        let width = Int(plan.width), height = Int(plan.height)
        let fps = CMTimeScale(plan.fps)

        let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoColorPropertiesKey: [
                AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_709_2,
                AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_709_2,
                AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_709_2,
            ],
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: Int(plan.averageBitrateBps),
                AVVideoExpectedSourceFrameRateKey: Int(plan.fps),
                AVVideoMaxKeyFrameIntervalKey: Int(plan.fps) * 2,
                AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
            ],
        ])
        input.expectsMediaDataInRealTime = false
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(
            assetWriterInput: input,
            sourcePixelBufferAttributes: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
                kCVPixelBufferWidthKey as String: width,
                kCVPixelBufferHeightKey as String: height,
            ])
        guard writer.canAdd(input) else { throw ExportError.cannotStart(nil) }
        writer.add(input)
        guard writer.startWriting() else {
            throw ExportError.cannotStart(writer.error?.localizedDescription)
        }
        writer.startSession(atSourceTime: .zero)

        do {
            var previous: CVPixelBuffer?
            for index in 0..<plan.totalFrameCount {
                try Task.checkCancellation()
                let buffer: CVPixelBuffer
                if index >= plan.replayFrameCount, let previous {
                    // 完成後の静止（ホールド）は同じ画を繰り返すだけ。
                    buffer = previous
                } else {
                    buffer = try pixelBuffer(from: job.renderFrame(index: index),
                                             pool: adaptor.pixelBufferPool)
                }
                while !input.isReadyForMoreMediaData {
                    try await Task.sleep(for: .milliseconds(2))
                }
                let time = CMTime(value: CMTimeValue(index), timescale: fps)
                guard adaptor.append(buffer, withPresentationTime: time) else {
                    throw ExportError.writeFailed(writer.error?.localizedDescription)
                }
                previous = buffer
                if index % 4 == 0 {
                    await progress(Double(index + 1) / Double(plan.totalFrameCount))
                }
            }
            try Task.checkCancellation()
            input.markAsFinished()
            writer.endSession(atSourceTime: CMTime(value: CMTimeValue(plan.totalFrameCount), timescale: fps))
            await writer.finishWriting()
            guard writer.status == .completed else {
                throw ExportError.writeFailed(writer.error?.localizedDescription)
            }
            await progress(1)
            return url
        } catch {
            if writer.status == .writing { writer.cancelWriting() }
            try? FileManager.default.removeItem(at: url)
            throw error
        }
    }

    /// コアの RGBA8（左上原点・行優先）をエンコーダ用の BGRA ピクセルバッファへ詰め替える。
    private static func pixelBuffer(from raster: RasterImageF, pool: CVPixelBufferPool?) throws -> CVPixelBuffer {
        let width = Int(raster.width), height = Int(raster.height)
        guard let pool, raster.rgba.count == width * height * 4 else { throw ExportError.pixelBuffer }
        var created: CVPixelBuffer?
        guard CVPixelBufferPoolCreatePixelBuffer(nil, pool, &created) == kCVReturnSuccess,
              let buffer = created,
              CVPixelBufferGetWidth(buffer) == width, CVPixelBufferGetHeight(buffer) == height
        else { throw ExportError.pixelBuffer }

        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { throw ExportError.pixelBuffer }
        let status = raster.rgba.withUnsafeBytes { (src: UnsafeRawBufferPointer) -> vImage_Error in
            var source = vImage_Buffer(data: UnsafeMutableRawPointer(mutating: src.baseAddress),
                                       height: vImagePixelCount(height),
                                       width: vImagePixelCount(width),
                                       rowBytes: width * 4)
            var destination = vImage_Buffer(data: base,
                                            height: vImagePixelCount(height),
                                            width: vImagePixelCount(width),
                                            rowBytes: CVPixelBufferGetBytesPerRow(buffer))
            let rgbaToBGRA: [UInt8] = [2, 1, 0, 3]
            return vImagePermuteChannels_ARGB8888(&source, &destination, rgbaToBGRA,
                                                  vImage_Flags(kvImageNoFlags))
        }
        guard status == kvImageNoError else { throw ExportError.pixelBuffer }
        return buffer
    }
}
