import CoreGraphics
import UIKit

extension UIImage {
    /// コアが描いた不透明 RGBA8 画像（左上原点・行優先）を表示用に包むだけ。
    convenience init?(raster: RasterImageF) {
        let width = Int(raster.width), height = Int(raster.height)
        guard width > 0, height > 0, raster.rgba.count == width * height * 4,
              let provider = CGDataProvider(data: raster.rgba as CFData),
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let cgImage = CGImage(
                  width: width, height: height,
                  bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                  space: space,
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                  provider: provider, decode: nil,
                  shouldInterpolate: true, intent: .defaultIntent)
        else { return nil }
        self.init(cgImage: cgImage)
    }
}
