import GlassModels
import SwiftUI

struct TeammatesView: View {
    let conversation: ConversationModel
    let open: (Teammate) -> Void

    var body: some View {
        List {
            if conversation.agents.isEmpty {
                ContentUnavailableView("No teammates", systemImage: "person.2",
                                       description: Text("Agents appear here when this conversation delegates work."))
            }
            ForEach(conversation.agents) { agent in
                Button { open(agent) } label: {
                    VStack(alignment: .leading, spacing: Design.small) {
                        HStack {
                            Text(agent.title).font(.body.weight(.medium))
                            Spacer()
                            Text(agent.agentStatusTitle).font(.caption)
                                .foregroundStyle(agent.isWorking ? Design.accent : Design.muted)
                        }
                        if let result = agent.isWorking ? agent.activity : agent.result ?? agent.task, !result.isEmpty {
                            Text(agentHeadline(result)).font(.footnote).foregroundStyle(Design.muted).lineLimit(1)
                        }
                    }
                    .padding(.leading, CGFloat(max(0, min(agent.depth ?? 1, 4) - 1)) * Design.gap)
                    .frame(minHeight: Design.control)
                    .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .task { await conversation.prefetchAgent(agent) }
                .accessibilityIdentifier("teammates.row")
            }
        }
        .navigationTitle("Teammates")
        .refreshable { await conversation.refreshAgents() }
    }
}
