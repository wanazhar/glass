import Foundation
import GlassModels
import Testing

@Suite struct ComponentsTests {
    @Test func everyPromptAdvertisesTheIOSRenderCapabilities() throws {
        let json = ClientRender.ios.json
        #expect(json["name"] == "ios")
        #expect(json["schema"]?.doubleValue == 1)
        #expect(json["components"]?.arrayValue?.compactMap(\.stringValue) == ReplyComponent.types)
        #expect(ReplyComponent.types.count == 35)
    }

    @Test func parsesAComponentEvenWhenStreamingSplitItsLine() throws {
        let source = "{\"type\":\"table\",\"v\":1,\"fallback\":\"| a |\",\n\"data\":{\"rows\":[]}}"
        let component = try #require(ReplyComponent(source: source))
        #expect(component.type == "table")
        #expect(component.isDrawable())
        #expect(component.fallbackText == "| a |")
    }

    @Test func unknownTypesAndNewerSchemasShowTheirFallback() throws {
        let hologram = try #require(ReplyComponent(source: #"{"type":"hologram","fallback":"Plain"}"#))
        #expect(!hologram.isDrawable())
        #expect(hologram.fallbackText == "Plain")
        let future = try #require(ReplyComponent(source: #"{"type":"table","v":2,"fallback":"Rows"}"#))
        #expect(!future.isDrawable())
        #expect(ReplyComponent(source: "{not json") == nil)
        #expect(ReplyComponent(source: #"{"v":1}"#) == nil)
    }

    @Test func exampleDataIsMarkedInTheFallback() throws {
        let rates = try #require(ReplyComponent(source: #"{"type":"currency","example":true,"fallback":"1 USD = 0.9 EUR"}"#))
        #expect(rates.fallbackText.hasPrefix("_Example data, not live._"))
    }

    @Test func answersBecomeAListForTheMessageBox() {
        #expect(ReplyComponent.answer(title: "Trip", [("Name", "Ada"), ("Size", "M")]) == "Trip:\n- Name: Ada\n- Size: M")
    }

    @Test func fieldsReadAsDisplayText() {
        #expect(JSONValue.number(3).text == "3")
        #expect(JSONValue.number(2.5).text == "2.5")
        #expect(JSONValue.string("7").number == 7)
    }
}
