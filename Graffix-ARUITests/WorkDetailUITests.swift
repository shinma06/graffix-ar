import XCTest

/// フェーズ 1.5 の作品詳細（シーンシミュレーション・タイムラプス）の UI テスト。
/// AR は Simulator で動かないので、決定的な保存作品をシードして
/// 「作品」タブから詳細を開き、操作に応じてプレビューと進捗が変わることを確かめる。
@MainActor
final class WorkDetailUITests: XCTestCase {
    private var app: XCUIApplication!

    // MARK: - Tests

    func testWorksListShowsSeededWorks() {
        launchAndOpenWorksTab()
        let v3 = app.buttons["worksList.row.\(WorkFixtures.timedV3.id)"]
        let v2 = app.buttons["worksList.row.\(WorkFixtures.legacyV2.id)"]
        XCTAssertTrue(v3.waitForExistence(timeout: 10))
        XCTAssertTrue(v2.exists)
        XCTAssertTrue(app.staticTexts[WorkFixtures.timedV3.title].exists)
        XCTAssertTrue(app.staticTexts[WorkFixtures.legacyV2.title].exists)
        // 新しい作品が上。
        XCTAssertLessThan(v3.frame.minY, v2.frame.minY)
        attachScreenshot("01-works-list")
    }

    func testSceneOptionsUpdatePreview() {
        launchAndOpenWorksTab()
        openWork(WorkFixtures.timedV3)

        let preview = app.images["workDetail.scene.preview"]
        XCTAssertTrue(preview.waitForExistence(timeout: 10))
        let caption = app.staticTexts["workDetail.scene.caption"]
        XCTAssertTrue(caption.waitForExistence(timeout: 10))
        // 初回レンダリングが終わると視野の幅（コアの計算値）が入る。
        waitFor(caption, "初回レンダリング") { !$0.label.contains("視野の幅 0.0 m") }
        var image = stableImage(of: preview)
        attachScreenshot("02-scene-default")

        selectSegment("workDetail.scene.lighting", "夜（街灯）")
        image = changedImage(of: preview, from: image, after: "照明 → 夜（街灯）")
        attachScreenshot("03-scene-night")

        selectSegment("workDetail.scene.material", "レンガ")
        image = changedImage(of: preview, from: image, after: "素材 → レンガ")

        let materialSwatch = app.buttons["workDetail.scene.wallColor.material"]
        XCTAssertTrue(materialSwatch.waitForExistence(timeout: 5))
        XCTAssertTrue(materialSwatch.isSelected, "初期状態は「素材の色」")
        let swatch = app.buttons["workDetail.scene.wallColor.black"]
        XCTAssertTrue(swatch.exists)
        swatch.tap()
        image = changedImage(of: preview, from: image, after: "壁色 → ブラック")
        XCTAssertTrue(swatch.isSelected, "選んだ壁色が選択状態になる")
        XCTAssertFalse(materialSwatch.isSelected)
        attachScreenshot("04-scene-night-brick-color")

        materialSwatch.tap()
        image = changedImage(of: preview, from: image, after: "壁色 → 素材の色")
        XCTAssertTrue(materialSwatch.isSelected, "「素材の色」に戻せる")
        swatch.tap()
        image = changedImage(of: preview, from: image, after: "壁色 → ブラック（再）")

        let captionBefore = caption.label
        let slider = app.sliders["workDetail.scene.distance"]
        XCTAssertTrue(slider.waitForExistence(timeout: 5))
        slider.adjust(toNormalizedSliderPosition: 0.9)
        image = changedImage(of: preview, from: image, after: "視距離スライダー")
        XCTAssertNotEqual(caption.label, captionBefore, "視距離・視野の幅の表示が更新される")
        XCTAssertFalse(caption.label.hasPrefix("3.0 m"), caption.label)
        attachScreenshot("05-scene-far")
    }

    func testTimelapsePlayScrubAndSpeed() {
        launchAndOpenWorksTab()
        openWork(WorkFixtures.timedV3)
        selectSegment("workDetail.mode", "タイムラプス")

        let preview = app.images["workDetail.timelapse.preview"]
        let status = app.staticTexts["workDetail.timelapse.status"]
        let play = app.buttons["workDetail.timelapse.playPause"]
        XCTAssertTrue(status.waitForExistence(timeout: 10))
        XCTAssertTrue(play.exists)
        XCTAssertFalse(app.staticTexts["workDetail.timelapse.legacyNote"].exists,
                       "記録時刻のある v3 作品には旧形式の注記を出さない")

        // 初期表示は最終フレーム（完成画像）。
        let initial = progress(status)
        XCTAssertEqual(initial.strokesShown, 3)
        XCTAssertEqual(initial.strokeCount, 3)
        XCTAssertGreaterThan(initial.duration, 5)
        XCTAssertEqual(initial.seconds, initial.duration, accuracy: 0.05)
        let duration = initial.duration
        // 動画書き出しボタンは存在し押せる状態（エンコード自体は UI テストの対象外）。
        let export = app.buttons["workDetail.timelapse.export"]
        XCTAssertTrue(export.waitForExistence(timeout: 5))
        XCTAssertTrue(export.isEnabled)
        var image = stableImage(of: preview)
        attachScreenshot("06-timelapse-final")

        // シーク: 途中の位置に移動すると途中のフレームになる。
        let scrubber = app.sliders["workDetail.timelapse.scrubber"]
        XCTAssertTrue(scrubber.exists)
        scrubber.adjust(toNormalizedSliderPosition: 0.25)
        waitFor(status, "シーク後の進捗") { self.parse($0.label).map { $0.seconds < duration * 0.5 } ?? false }
        let scrubbed = progress(status)
        XCTAssertGreaterThan(scrubbed.seconds, 0)
        XCTAssertLessThan(scrubbed.strokesShown, 3)
        image = changedImage(of: preview, from: image, after: "シーク")
        attachScreenshot("07-timelapse-scrubbed")

        // ×1 で再生 → 進捗が進む → 一時停止で止まる。
        selectSegment("workDetail.timelapse.speed", "×1")
        play.tap()
        waitFor(play, "再生中") { $0.label == "一時停止" }
        waitFor(status, "×1 再生で進捗が進む") {
            self.parse($0.label).map { $0.seconds > scrubbed.seconds + 0.3 } ?? false
        }
        play.tap()
        waitFor(play, "一時停止") { $0.label == "再生" }
        let paused = progress(status)
        XCTAssertGreaterThan(paused.seconds, scrubbed.seconds)
        XCTAssertLessThan(paused.seconds, duration, "×1 ではまだ最後まで進まない")
        Thread.sleep(forTimeInterval: 0.6)
        XCTAssertEqual(progress(status).seconds, paused.seconds, accuracy: 0.001, "一時停止中は進まない")
        image = changedImage(of: preview, from: image, after: "×1 再生")
        attachScreenshot("08-timelapse-paused")

        // ×8 に変えて再生 → 残り（×1 なら数秒）がすぐに終わる。
        selectSegment("workDetail.timelapse.speed", "×8")
        let remaining = duration - paused.seconds
        XCTAssertGreaterThan(remaining, 3)
        let start = Date()
        play.tap()
        waitFor(status, "×8 再生で最後まで進む", timeout: remaining) {
            self.parse($0.label).map { abs($0.seconds - duration) < 0.05 } ?? false
        }
        let elapsed = Date().timeIntervalSince(start)
        XCTAssertLessThan(elapsed, remaining, "×8 は等速より速く終わる（残り \(remaining) 秒を \(elapsed) 秒）")
        waitFor(play, "再生終了で自動停止") { $0.label == "再生" }
        XCTAssertEqual(progress(status).strokesShown, 3)
        _ = changedImage(of: preview, from: image, after: "×8 再生")
        attachScreenshot("09-timelapse-finished")
    }

    func testLegacyV2WorkScenesAndTimelapse() {
        launchAndOpenWorksTab()
        openWork(WorkFixtures.legacyV2)

        let caption = app.staticTexts["workDetail.scene.caption"]
        XCTAssertTrue(caption.waitForExistence(timeout: 10))
        waitFor(caption, "初回レンダリング") { !$0.label.contains("視野の幅 0.0 m") }
        let scenePreview = app.images["workDetail.scene.preview"]
        let sceneImage = stableImage(of: scenePreview)
        selectSegment("workDetail.scene.lighting", "夕方")
        _ = changedImage(of: scenePreview, from: sceneImage, after: "照明 → 夕方")
        attachScreenshot("10-legacy-scene-dusk")

        selectSegment("workDetail.mode", "タイムラプス")
        let status = app.staticTexts["workDetail.timelapse.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["workDetail.timelapse.legacyNote"].exists,
                      "時刻のない旧形式の注記が出る")
        let initial = progress(status)
        XCTAssertEqual(initial.strokeCount, 2)
        XCTAssertGreaterThan(initial.duration, 0)

        // スライダーの端への調整は数 % ずれることがあるので「先頭付近」で判定する。
        app.sliders["workDetail.timelapse.scrubber"].adjust(toNormalizedSliderPosition: 0)
        waitFor(status, "先頭付近へシーク") {
            self.parse($0.label).map { $0.seconds < initial.duration * 0.3 } ?? false
        }
        let preview = app.images["workDetail.timelapse.preview"]
        let startImage = stableImage(of: preview)
        attachScreenshot("11-legacy-timelapse-start")

        let play = app.buttons["workDetail.timelapse.playPause"]
        play.tap()
        waitFor(status, "一定速度の再生が最後まで進む", timeout: 15) {
            self.parse($0.label).map { abs($0.seconds - initial.duration) < 0.05 } ?? false
        }
        waitFor(play, "再生終了で自動停止") { $0.label == "再生" }
        _ = changedImage(of: preview, from: startImage, after: "旧形式の再生")
        attachScreenshot("12-legacy-timelapse-finished")
    }

    // TEMP PROBE (revert before merge): verifies the CI retry / timeout path.
    func testZZProbe1HangOnce() {
        let host = ProcessInfo.processInfo.environment["SIMULATOR_HOST_HOME"] ?? NSTemporaryDirectory()
        let marker = URL(fileURLWithPath: host).appendingPathComponent("probe-hang-once")
        if !FileManager.default.fileExists(atPath: marker.path) {
            FileManager.default.createFile(atPath: marker.path, contents: Data())
            Thread.sleep(forTimeInterval: 600)
        }
    }

    // TEMP PROBE (revert before merge): a deterministic failure must still fail the job.
    func testZZProbe2AlwaysFails() {
        XCTFail("probe: deterministic failure")
    }

    // MARK: - Navigation

    private func launchAndOpenWorksTab() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments += [WorkFixtures.launchArgument]
        app.launchEnvironment[WorkFixtures.environmentKey] = WorkFixtures.environmentPayload()
        app.launch()
        let tab = app.tabBars.buttons["作品"].firstMatch
        XCTAssertTrue(tab.waitForExistence(timeout: 20), "作品タブ")
        tab.tap()
        XCTAssertTrue(app.navigationBars["作品"].waitForExistence(timeout: 10))
    }

    private func openWork(_ fixture: WorkFixtures.Fixture) {
        let row = app.buttons["worksList.row.\(fixture.id)"]
        XCTAssertTrue(row.waitForExistence(timeout: 10), fixture.title)
        row.tap()
        XCTAssertTrue(app.navigationBars[fixture.title].waitForExistence(timeout: 10))
    }

    private func selectSegment(_ pickerID: String, _ title: String) {
        let segment = app.segmentedControls[pickerID].buttons[title]
        XCTAssertTrue(segment.waitForExistence(timeout: 5), "\(pickerID) / \(title)")
        segment.tap()
        waitFor(segment, "\(title) を選択") { $0.isSelected }
    }

    // MARK: - Waiting

    private func waitFor(
        _ element: XCUIElement,
        _ what: String,
        timeout: TimeInterval = 10,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @escaping (XCUIElement) -> Bool
    ) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if element.exists && condition(element) { return }
            Thread.sleep(forTimeInterval: 0.1)
        }
        XCTFail("待機タイムアウト: \(what)（\(element.exists ? element.label : "要素なし")）",
                file: file, line: line)
    }

    /// 連続 2 回のスクリーンショットが一致するまで待つ（レンダリング完了）。
    /// 遅い CI ランナーでは 1 回の撮影に数秒かかるので、時間切れでも最低
    /// `minComparisons` 回は比較してから諦める。安定しなければ nil。
    private func stableImage(of element: XCUIElement, timeout: TimeInterval,
                             minComparisons: Int) -> Data? {
        let deadline = Date().addingTimeInterval(timeout)
        var last = element.screenshot().pngRepresentation
        var comparisons = 0
        while comparisons < minComparisons || Date() < deadline {
            Thread.sleep(forTimeInterval: 0.3)
            let next = element.screenshot().pngRepresentation
            comparisons += 1
            if next == last { return next }
            last = next
        }
        return nil
    }

    /// 安定したプレビュー画像。安定しなければテストを失敗させる。
    private func stableImage(of element: XCUIElement,
                             file: StaticString = #filePath, line: UInt = #line) -> Data {
        let start = Date()
        if let image = stableImage(of: element, timeout: 30, minComparisons: 5) { return image }
        XCTFail("プレビューが安定しない（\(Int(Date().timeIntervalSince(start))) 秒）", file: file, line: line)
        return element.screenshot().pngRepresentation
    }

    /// プレビューが `previous` から変わり、安定するまで待つ。
    /// 描画中の撮影で安定判定に失敗しても、時間内（最低 `minAttempts` 回）は撮り直す。
    private func changedImage(of element: XCUIElement, from previous: Data, after action: String,
                              timeout: TimeInterval = 60, minAttempts: Int = 3,
                              file: StaticString = #filePath, line: UInt = #line) -> Data {
        let start = Date()
        let deadline = start.addingTimeInterval(timeout)
        var attempts = 0
        var sawStable = false
        while attempts < minAttempts || Date() < deadline {
            attempts += 1
            guard let image = stableImage(of: element, timeout: 10, minComparisons: 3) else { continue }
            sawStable = true
            if image != previous { return image }
        }
        let elapsed = Date().timeIntervalSince(start)
        let reason = sawStable ? "プレビュー画像が更新されない" : "プレビューが安定しない"
        XCTFail("\(reason): \(action)（\(attempts) 回・\(Int(elapsed)) 秒）", file: file, line: line)
        return previous
    }

    // MARK: - Progress

    private struct Progress {
        var strokesShown: Int
        var strokeCount: Int
        var seconds: Double
        var duration: Double
    }

    /// "ストローク 2 / 3 ・ 4.1 / 9.6 秒"
    private func parse(_ label: String) -> Progress? {
        let regex = /ストローク (\d+) \/ (\d+) ・ ([0-9.]+) \/ ([0-9.]+) 秒/
        guard let m = label.firstMatch(of: regex),
              let shown = Int(m.1), let count = Int(m.2),
              let seconds = Double(m.3), let duration = Double(m.4) else { return nil }
        return Progress(strokesShown: shown, strokeCount: count, seconds: seconds, duration: duration)
    }

    private func progress(_ status: XCUIElement,
                          file: StaticString = #filePath, line: UInt = #line) -> Progress {
        guard let p = parse(status.label) else {
            XCTFail("進捗表示を解釈できない: \(status.label)", file: file, line: line)
            return Progress(strokesShown: 0, strokeCount: 0, seconds: 0, duration: 0)
        }
        return p
    }

    private func attachScreenshot(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
