import SwiftUI

/// アプリのルート — 描く / マップ / 作品 の 3 タブ。
/// ローカル作品ストアと位置情報サービスはタブ間で共有する。
struct RootView: View {
    enum Tab {
        case draw
        case map
        case works
    }

    @State private var tab = Tab.draw
    @State private var drawViewModel = GraffixViewModel(engine: Engine())
    @State private var worksStore = LocalWorkStore()
    @State private var location = LocationService()
    /// 作品タブ → マップタブへの「このピンにフォーカス」要求。
    @State private var mapFocus: MapSpotF?

    var body: some View {
        TabView(selection: $tab) {
            ContentView(viewModel: drawViewModel, worksStore: worksStore, location: location)
                .tabItem { Label("描く", systemImage: "paintbrush.fill") }
                .tag(Tab.draw)

            MapScreen(
                worksStore: worksStore,
                location: location,
                workPinProvider: MockWorkPinProvider(),
                focusRequest: $mapFocus
            )
            .tabItem { Label("マップ", systemImage: "map.fill") }
            .tag(Tab.map)

            WorksListView(store: worksStore) { work in
                if let spot = worksStore.mapSpot(for: work) {
                    mapFocus = spot
                    tab = .map
                }
            }
            .tabItem { Label("作品", systemImage: "photo.stack.fill") }
            .tag(Tab.works)
        }
    }
}
