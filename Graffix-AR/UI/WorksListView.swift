import SwiftUI

/// 作品タブ — ローカル保存した自分の作品の一覧。
/// 行タップで作品詳細（シーンシミュレーション・タイムラプス）へ、
/// ピンのボタンでその作品の位置にマップをフォーカスする。
struct WorksListView: View {
    let store: LocalWorkStore
    /// 位置を持つ作品が選ばれたときに呼ばれる（マップタブへの遷移は親が行う）。
    let onShowOnMap: (LocalWorkRecord) -> Void

    @State private var showingNoLocationAlert = false

    var body: some View {
        NavigationStack {
            List {
                ForEach(store.works) { work in
                    NavigationLink(value: work) {
                        row(for: work)
                    }
                }
                .onDelete { offsets in
                    for index in offsets {
                        store.delete(store.works[index])
                    }
                }
            }
            .listStyle(.plain)
            .navigationTitle("作品")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: LocalWorkRecord.self) { work in
                WorkDetailView(record: work, store: store, onShowOnMap: onShowOnMap)
            }
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

    private func row(for work: LocalWorkRecord) -> some View {
        HStack(spacing: 12) {
            Button {
                if work.coordinate != nil {
                    onShowOnMap(work)
                } else {
                    showingNoLocationAlert = true
                }
            } label: {
                Image(systemName: work.coordinate != nil
                      ? "mappin.circle.fill" : "mappin.slash.circle")
                    .font(.title3)
                    .foregroundStyle(work.coordinate != nil ? .orange : .secondary)
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(work.coordinate != nil ? "マップで見る" : "位置情報なし")
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
