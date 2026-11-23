import Foundation
import GlassModels
import Testing

@Suite struct SettingsTests {
    static let json = #"""
    {"revision":"r1","scope":"global","applies":"new_conversations",
     "defaults":{"provider":"openai","base_url":"https://api.openai.com/v1","model":"gpt-5","max_tokens":32000,"reasoning_budget":null,"cost_limit":2.5,"adaptive_pacing":true,"image_input":false,"system_prompt":"Be brief.","skills_dir":null,"permission_profile":"workspace-edit","sandbox":"workspace","sandbox_managed":true,"mods":["redact"]},
     "active_connection":"c1",
     "connections":[{"id":"c1","name":"Work","provider":"anthropic","endpoint":"https://api.anthropic.com","model":"claude","has_credential":true}],
     "available_mods":["redact","overlay"],
     "extensions":[{"id":"git","name":"git","description":"Git tools","enabled":true}],
     "profiles":[{"name":"zeta","model":"m2","instructions":"","adaptive_pacing":false,"workspaces":[]},{"name":"alpha","model":"m1","instructions":"Plan first","adaptive_pacing":true,"workspaces":["/w"]}],
     "skills":[{"name":"review","description":"Review code"}]}
    """#

    @Test func decodesTheSnapshot() throws {
        let s = try JSONDecoder().decode(SettingsSnapshot.self, from: Data(Self.json.utf8))
        #expect(s.revision == "r1")
        #expect(s.defaults.maxTokens == 32000 && s.defaults.reasoningBudget == nil && s.defaults.costLimit == 2.5)
        #expect(s.defaults.permissionProfile == "workspace-edit" && s.defaults.sandboxManaged)
        #expect(s.activeConnection == "c1" && s.connections.first?.hasCredential == true)
        #expect(s.availableMods == ["redact", "overlay"] && s.defaults.mods == ["redact"])
        #expect(s.extensions.first?.enabled == true)
        #expect(s.profiles.map(\.name) == ["alpha", "zeta"])
        #expect(s.profiles.first?.workspaces == ["/w"])
        #expect(s.skills.first?.name == "review")
        #expect(s.signIn == nil)
    }

    @Test func changesEncodeExactly() {
        #expect(SettingsChange.preference(key: "max_tokens", value: .number(4096)).body(revision: "r1")
                == .object(["revision": "r1", "action": "preference", "key": "max_tokens", "value": .number(4096)]))
        #expect(SettingsChange.preference(key: "reasoning_budget", value: .null).body(revision: "r1")["value"] == .null)
        #expect(SettingsChange.extensionEnabled(id: "git", enabled: false).body(revision: "r")
                == .object(["revision": "r", "action": "extension", "id": "git", "enabled": false]))
        #expect(SettingsChange.useConnection(id: "").body(revision: "r")
                == .object(["revision": "r", "action": "connection-use", "id": ""]))
        #expect(SettingsChange.connectionModel(id: "c1", model: "m").body(revision: "r")
                == .object(["revision": "r", "action": "connection-model", "id": "c1", "model": "m"]))
        #expect(SettingsChange.removeConnection(id: "c1").body(revision: "r")
                == .object(["revision": "r", "action": "connection-remove", "id": "c1", "confirmed": true]))
        #expect(SettingsChange.addConnection(label: "L", provider: "anthropic", endpoint: "https://e", model: "m", secret: "k").body(revision: "r")
                == .object(["revision": "r", "action": "connection-add", "label": "L", "provider": "anthropic",
                            "endpoint": "https://e", "model": "m", "secret": "k"]))
        #expect(SettingsChange.startSignIn.body(revision: "r") == .object(["revision": "r", "action": "oauth"]))
        #expect(SettingsChange.completeSignIn(attempt: "a1", callbackURL: "http://localhost/cb", model: "m", label: "").body(revision: "r")
                == .object(["revision": "r", "action": "callback", "args": "a1", "model": "m", "label": "",
                            "secret": "http://localhost/cb"]))
        #expect(SettingsChange.updateProfile(name: "p", instructions: "x").body(revision: "r")
                == .object(["revision": "r", "action": "profile", "name": "p", "instructions": "x"]))
        #expect(SettingsChange.createProfile(name: "p").body(revision: "r")
                == .object(["revision": "r", "action": "profile-create", "name": "p"]))
        #expect(SettingsChange.deleteProfile(name: "p").body(revision: "r")
                == .object(["revision": "r", "action": "profile-delete", "name": "p", "confirmed": true]))
        #expect(SettingsChange.assignProfile(name: "p", workspace: "/w").body(revision: "r")
                == .object(["revision": "r", "action": "profile-assign", "name": "p", "workspace": "/w"]))
    }

    @Test func credentialChangesAreSecretAndConflictsAreRecognised() {
        #expect(SettingsChange.addConnection(label: "", provider: "", endpoint: "", model: "", secret: "k").carriesSecret)
        #expect(SettingsChange.completeSignIn(attempt: nil, callbackURL: "u", model: "", label: "").carriesSecret)
        #expect(!SettingsChange.startSignIn.carriesSecret)
        #expect(SettingsChange.isConflict("Settings changed elsewhere. Refresh before saving your changes."))
        #expect(SettingsChange.isConflict("Profiles changed elsewhere. Refresh before saving."))
        #expect(!SettingsChange.isConflict("Unknown provider"))
        #expect(SettingsChange.defaultEndpoint(provider: "anthropic") == "https://api.anthropic.com")
        #expect(SettingsChange.defaultEndpoint(provider: "openai") == "https://api.openai.com/v1")
    }
}
