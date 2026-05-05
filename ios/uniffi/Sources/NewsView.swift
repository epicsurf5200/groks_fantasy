import SwiftUI

struct NewsView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        NavigationStack {
            List(store.news, id: \.url) { n in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text("[\(n.source)]").font(.caption.bold()).foregroundStyle(.cyan)
                        Spacer()
                    }
                    if let url = URL(string: n.url), !n.url.isEmpty {
                        Link(n.title, destination: url).font(.body)
                    } else {
                        Text(n.title).font(.body)
                    }
                    if !n.summary.isEmpty {
                        Text(n.summary).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("News")
            .padding(.top, 60)
        }
    }
}
