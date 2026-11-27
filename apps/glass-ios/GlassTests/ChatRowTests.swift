import Foundation
import GlassModels
import Testing
@testable import Glass

@Test func recordedSubagentToolCreatesAgentCard() throws {
    let page = try JSONDecoder().decode(HistoryPage.self, from: Data(#"""
    {"before":0,"records":[
      {"index":1,"record":{"k":"call","id":"batch","name":"subagent","args":"{}"}},
      {"index":2,"record":{"k":"tool","id":"batch","name":"subagent","t":"Report"}}
    ]}
    """#.utf8))
    var timeline = Timeline()
    timeline.replaceHistory(page)
    guard case .agents(let entry)? = ChatRow.make(from: timeline).first else {
        Issue.record("The recorded subagent tool must be an agent card")
        return
    }
    #expect(entry.id == "batch")
}
