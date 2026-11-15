import MapKit
import SwiftUI

/// マップ画面 — 公認ウォール、他人の作品、自分の作品のピンを出す。
/// 絞り込みと近い順の並びはコア（`visibleMapSpots`）が決める。
/// 位置情報の権限が無くても動作し、データの外接矩形を初期表示する。
struct MapScreen: View {
    /// 作品タブなどからの「このピンにフォーカス」要求。
    @Binding var focusRequest: MapSpotF?

    @State private var viewModel: MapViewModel
    @State private var cameraPosition: MapCameraPosition = .automatic
    @State private var selection: String?
    @State private var detail: MapSpotF?
    @State private var showList = false

    init(
        worksStore: LocalWorkStore,
        location: LocationService,
        workPinProvider: WorkPinProvider,
        focusRequest: Binding<MapSpotF?>
    ) {
        _focusRequest = focusRequest
        _viewModel = State(
            initialValue: MapViewModel(
                worksStore: worksStore,
                location: location,
                workPinProvider: workPinProvider
            )
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            Picker("表示", selection: $showList) {
                Text("マップ").tag(false)
                Text("一覧").tag(true)
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)

            if showList {
                spotList
            } else {
                mapView
            }
        }
        .task { await viewModel.load() }
        .onAppear { consumeFocus() }
        .onChange(of: focusRequest) { consumeFocus() }
        .onChange(of: selection) { _, id in
            if let id, let spot = viewModel.spots.first(where: { $0.id == id }) {
                detail = spot
            }
        }
        .sheet(item: $detail, onDismiss: { selection = nil }) { spot in
            SpotDetailView(spot: spot, distanceText: viewModel.distanceText(to: spot))
                .presentationDetents([.fraction(0.38), .medium])
        }
    }

    /// フォーカス要求を消費: カメラを寄せてピンを選択（詳細シートまで繋ぐ）。
    private func consumeFocus() {
        guard let spot = focusRequest else { return }
        showList = false
        cameraPosition = .region(
            MKCoordinateRegion(
                center: spot.coordinate,
                latitudinalMeters: 800,
                longitudinalMeters: 800
            )
        )
        selection = spot.id
        focusRequest = nil
    }

    // MARK: - Map

    private var mapView: some View {
        Map(position: $cameraPosition, selection: $selection) {
            ForEach(viewModel.spots) { spot in
                Annotation(spot.title, coordinate: spot.coordinate) {
                    pinGlyph(for: spot.kind)
                }
                .tag(spot.id)
            }
            UserAnnotation()
        }
        .mapControls {
            MapUserLocationButton()
            MapCompass()
        }
        .onChange(of: viewModel.loaded) { _, loaded in
            // 位置権限が無くてもデータ全体が見える初期位置にする。
            // フォーカス要求や手動選択が済んでいるときは上書きしない。
            guard loaded, selection == nil, let region = viewModel.boundingRegion else { return }
            cameraPosition = .region(
                MKCoordinateRegion(
                    center: region.center,
                    span: MKCoordinateSpan(
                        latitudeDelta: region.latDelta,
                        longitudeDelta: region.lonDelta
                    )
                )
            )
        }
    }

    // MARK: - List

    private var spotList: some View {
        List(viewModel.sortedSpots) { spot in
            Button {
                detail = spot
            } label: {
                HStack(spacing: 12) {
                    pinGlyph(for: spot.kind)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(spot.title)
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.primary)
                        Text(kindLabel(spot.kind))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if let text = viewModel.distanceText(to: spot) {
                        Text(text)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        .listStyle(.plain)
    }

    // MARK: - Pin appearance

    private func pinGlyph(for kind: MapSpotKindF) -> some View {
        Image(systemName: pinIcon(kind))
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(.white)
            .padding(6)
            .background(pinColor(kind), in: Circle())
            .overlay(Circle().stroke(.white, lineWidth: 1.5))
            .shadow(radius: 2)
    }

    private func pinIcon(_ kind: MapSpotKindF) -> String {
        switch kind {
        case .legalWall: "checkmark.seal.fill"
        case .workMine: "person.fill"
        case .workOther: "paintpalette.fill"
        }
    }

    private func pinColor(_ kind: MapSpotKindF) -> Color {
        switch kind {
        case .legalWall: .green
        case .workMine: .orange
        case .workOther: .purple
        }
    }

    private func kindLabel(_ kind: MapSpotKindF) -> String {
        switch kind {
        case .legalWall: "公認ウォール"
        case .workMine: "自分の作品"
        case .workOther: "みんなの作品"
        }
    }
}

/// ピン選択時の詳細シート。
/// スポット: 名称・種別・ルート案内（Apple マップへ）。
/// 作品: タイトル・作者・外部 SNS リンク・「レンズで見る」プレースホルダ。
private struct SpotDetailView: View {
    let spot: MapSpotF
    let distanceText: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                kindBadge
                Spacer()
                if let distanceText {
                    Label(distanceText, systemImage: "figure.walk")
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }

            Text(spot.title)
                .font(.title3.weight(.bold))
            if let subtitle = spot.subtitle {
                Text(subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            VStack(spacing: 10) {
                Button { openDirections() } label: {
                    Label("ルート案内", systemImage: "arrow.triangle.turn.up.right.diamond.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)

                if spot.kind == .workOther, let urlString = spot.authorUrl,
                   let url = URL(string: urlString) {
                    Link(destination: url) {
                        Label("作者のSNSを見る", systemImage: "safari")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                }

                if spot.kind != .legalWall {
                    Button {} label: {
                        Label("レンズで見る", systemImage: "camera.viewfinder")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .disabled(true)
                    Text("レンズ機能はフェーズ2で実装予定です")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
        }
        .padding(20)
    }

    private var kindBadge: some View {
        let (label, color): (String, Color) = switch spot.kind {
        case .legalWall: ("公認ウォール", .green)
        case .workMine: ("自分の作品", .orange)
        case .workOther: ("みんなの作品", .purple)
        }
        return Text(label)
            .font(.caption.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .background(color.opacity(0.15), in: Capsule())
    }

    /// Apple マップアプリで徒歩ルートを開く。
    private func openDirections() {
        let item = MKMapItem(placemark: MKPlacemark(coordinate: spot.coordinate))
        item.name = spot.title
        item.openInMaps(launchOptions: [
            MKLaunchOptionsDirectionsModeKey: MKLaunchOptionsDirectionsModeWalking
        ])
    }
}
