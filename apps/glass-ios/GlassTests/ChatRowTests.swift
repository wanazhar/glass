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

@Test func teammateCallsWithoutChildAgentsKeepTheirResultsInTheWorkLog() throws {
    let page = try JSONDecoder().decode(HistoryPage.self, from: Data(#"""
    {"before":0,"records":[
      {"index":1,"record":{"k":"tool","id":"list","name":"teammate","t":"Two active agents"}},
      {"index":2,"record":{"k":"tool","id":"inspect","name":"teammate","t":"Working on replay"}},
      {"index":3,"record":{"k":"tool","id":"send","name":"teammate","t":"Message queued"}},
      {"index":4,"record":{"k":"tool","id":"spawn","name":"teammate","t":"Agent created"}}
    ]}
    """#.utf8))
    var timeline = Timeline()
    timeline.replaceHistory(page)
    let rows = ChatRow.make(from: timeline, agentCallIDs: ["spawn"])
    #expect(rows.count == 2)
    guard case .work(let work)? = rows.first, case .agents(let launch)? = rows.last else {
        Issue.record("Only the teammate call that launched a child should be an agent card")
        return
    }
    #expect(work.steps == [
        .tool(id: "list", name: "teammate", summary: "", output: "Two active agents", failed: false),
        .tool(id: "inspect", name: "teammate", summary: "", output: "Working on replay", failed: false),
        .tool(id: "send", name: "teammate", summary: "", output: "Message queued", failed: false),
    ])
    #expect(launch.id == "spawn")
}
