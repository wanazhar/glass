import Foundation
import GlassModels
import Testing

@Suite struct CommandsTests {
    static let catalogue: JSONValue = try! JSONDecoder().decode(JSONValue.self, from: Data(#"""
    {"commands":[
      {"name":"usage","description":"Token usage · local","hint":"","aliases":["cost"],"keywords":"tokens spend","primary":false,"group":"work","portable":true,"readonly":true,"form":{"fields":[],"confirm":false,"model_request":false}},
      {"name":"todo","description":"Checklist","hint":"[action]","aliases":[],"keywords":"tasks","primary":false,"group":"work","portable":true,"readonly":false,"form":{"fields":[{"key":"action","label":"add | complete | start | block | remove | clear","kind":"text"},{"key":"args","label":"Task","kind":"text"}],"confirm":false,"model_request":false}},
      {"name":"name","description":"Rename","hint":"","aliases":[],"keywords":"","primary":false,"group":"conversations","portable":true,"readonly":false,"form":{"fields":[{"key":"args","label":"Name","kind":"text"}],"confirm":false,"model_request":false}},
      {"name":"compact","description":"Compact context","hint":"","aliases":[],"keywords":"","primary":false,"group":"work","portable":true,"readonly":false,"form":{"fields":[{"key":"args","label":"Focus","kind":"textarea"},{"key":"limit","label":"Limit","kind":"number"},{"key":"site","label":"Site","kind":"url"},{"key":"secret","label":"Key","kind":"password"},{"key":"odd","label":"Odd","kind":"slider"}],"confirm":true,"model_request":true}},
      {"name":"settings","description":"Settings","primary":true,"group":"settings","portable":false,"readonly":false},
      {"description":"missing name"}
    ]}
    """#.utf8))

    var commands: [CommandDescriptor] { CommandCatalogue.decode(Self.catalogue) }

    @Test func decodesEntriesAndSkipsMalformedOnes() throws {
        #expect(commands.map(\.name) == ["usage", "todo", "name", "compact", "settings"])
        let usage = try #require(commands.first)
        #expect(usage.readonly && usage.portable)
        #expect(usage.description == "Token usage")
        #expect(usage.group == .work)
        let settings = commands[4]
        #expect(settings.primary && settings.form.fields.isEmpty && settings.group == .settings)
        #expect(CommandCatalogue.decode(.array(Self.catalogue["commands"]!.arrayValue!)).count == 5)
    }

    @Test func formSchemaMapsEveryFieldKind() {
        let compact = commands[3]
        #expect(compact.form.fields.map(\.kind) == [.textarea, .number, .url, .password, .text])
        #expect(compact.form.confirm && compact.form.modelRequest)
        #expect(compact.needsConfirmation && compact.cancellable)
        let action = commands[1].form.fields[0]
        #expect(action.choices == ["add", "complete", "start", "block", "remove", "clear"])
        #expect(commands[1].form.fields[1].choices == nil)
        #expect(CommandField.choiceTitle("saved-edit") == "Saved edit")
    }

    @Test func searchMatchesNameAliasKeywordsAndIgnoresSlash() {
        #expect(commands[0].matches("/cost"))
        #expect(commands[0].matches("SPEND"))
        #expect(!commands[0].matches("rename"))
        let groups = CommandCatalogue.grouped(commands, query: "")
        #expect(groups.map(\.0) == [.conversations, .work, .settings])
        #expect(CommandCatalogue.grouped(commands, query: "rename").map { $0.1.map(\.name) } == [["name"]])
    }

    @Test func actionVariantsHideOtherFieldsAndValuesAreStrings() {
        let todo = commands[1]
        #expect(todo.visibleFields(for: "clear").map(\.key) == ["action"])
        #expect(todo.visibleFields(for: "add").map(\.key) == ["action", "args"])
        #expect(todo.values(["action": "clear", "args": "stale"], confirmed: true)
                == .object(["action": "clear", "confirmed": true]))
        #expect(todo.values(["args": "Ship M4"], confirmed: false)
                == .object(["action": "add", "args": "Ship M4", "confirmed": false]))
        #expect(commands[3].values(["limit": "12"], confirmed: true)["limit"] == .string("12"))
        #expect(todo.runLabel(action: "add") == "Add")
        #expect(todo.runLabel(action: "saved-edit") == "Saved edit")
        #expect(todo.runLabel(action: "list") == "Refresh")
        #expect(commands[0].runLabel(action: nil) == "Refresh")
        #expect(commands[2].runLabel(action: nil) == "Apply")
        #expect(CommandDescriptor(name: "clear").runLabel(action: nil) == "Run")
        #expect(todo.inspectsOnOpen && commands[0].inspectsOnOpen && !commands[2].inspectsOnOpen)
    }

    @Test func commandEventsDecodeInOrder() throws {
        let lines = [
            #"{"t":"command_queued","request_id":"c1"}"#,
            #"{"t":"command_start","name":"compact","request_id":"c1"}"#,
            #"{"t":"command_result","request_id":"c1","result":{"title":"/compact","body":"done","format":"markdown"}}"#,
            #"{"t":"command_done"}"#,
            #"{"t":"command_result","request_id":"c2","error":"Queued command cancelled"}"#,
        ]
        let events = try lines.map { try JSONDecoder().decode(GlassEvent.self, from: Data($0.utf8)) }
        #expect(events[0] == .commandQueued(requestID: "c1"))
        #expect(events[1] == .commandStart(name: "compact"))
        guard case .commandResult("c1", let result?, nil) = events[2] else { Issue.record("result"); return }
        let output = CommandOutput(result)
        #expect(output.title == "/compact" && output.body == "done" && output.isMarkdown)
        #expect(events[3] == .commandDone)
        #expect(events[4] == .commandResult(requestID: "c2", result: nil, error: "Queued command cancelled"))
        var timeline = Timeline()
        for event in events { timeline.apply(event) }
        #expect(timeline.activity == nil)
    }

    @Test func resultLabelsMatchTheWebUI() {
        #expect(CommandLabel.title("cost_usd") == "Reported cost (USD)")
        #expect(CommandLabel.title("inputTokens") == "Input tokens")
        #expect(CommandLabel.title("total_tokens") == "Total tokens")
        #expect(CommandLabel.display(.null, key: "token_limit") == "Unlimited")
        #expect(CommandLabel.display(.null) == "Not reported")
        #expect(CommandLabel.display(.bool(true)) == "Yes")
        #expect(CommandLabel.display(.number(42)) == "42")
        #expect(CommandLabel.display(.number(0.25)) == "0.25")
        #expect(CommandLabel.identity(.object(["path": "a.txt", "id": "1"])) == "a.txt")
        #expect(CommandOutput(.object(["tokens": .number(3)])).body == .object(["tokens": .number(3)]))
    }
}
