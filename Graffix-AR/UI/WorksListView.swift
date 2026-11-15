import SwiftUI

/// 作品タブ — ローカル保存した自分の作品の簡易一覧。
/// 行タップでその作品の位置にマップをフォーカスする。
/// 本格的な作品管理（閲覧・再投影・シミュレーション）は後のパス。
struct WorksListView: View {
    let store: LocalWorkStore
    /// 位置を持つ作品が選ばれたときに呼ばれる（マップタブへの遷移は親が行う）。
    let onShowOnMap: (LocalWorkRecord) -> Void

    @State private var showingNoLocationAlert = false

    var body: some View {
        List {
            ForEach(store.works) { work in
                Button {
                    if work.coordinate != nil {
                        onShowOnMap(work)
                    } else {
                        showingNoLocationAlert = true
                    }
                } label: {
                    HStack(spacing: 12) {
                        Image(systemName: work.coordinate != nil
                              ? "mappin.circle.fill" : "mappin.slash.circle")
                            .font(.title3)
                            .foregroundStyle(work.coordinate != nil ? .orange : .secondary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(work.title)
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.primary)
                            Text(Date(timeIntervalSince1970: work.createdAt)
                                .formatted(.dateTime.year().month().day().hour().minute()))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                }
            }
            .onDelete { offsets in
                for index in offsets {
                    store.delete(store.works[index])
                }
            }
        }
        .listStyle(.plain)
        .overlay {
            if store.works.isEmpty {
                ContentUnavailableView(
                    "まだ作品がありません",
                    systemImage: "paintbrush.pointed",
                    description: Text("「描く」タブで描いて保存すると、ここに並びます")
                )
            }
        }
        .alert("位置情報がありません", isPresented: $showingNoLocationAlert) {
            Button("OK", role: .cancel) {}
        } message: {
            Text("この作品は位置情報なしで保存されたため、マップにピンがありません。")
        }
    }
}
