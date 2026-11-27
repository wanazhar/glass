import SwiftUI

struct Conversation: View {
    let screen: Screen
    let person: Person
    var openPerson: (Person) -> Void
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dynamicTypeSize) private var type
    @State private var stream = true
    @State private var steering = false
    @State private var retried = false
    var p: Palette { Palette(dark: scheme == .dark) }
    var group: Bool { [.sharedHome, .everyone, .empty].contains(screen) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if group { shared } else { individual }
            }
            .frame(maxWidth: 660)
            .padding(.horizontal, 22).padding(.vertical, 16)
            .frame(maxWidth: .infinity)
        }
        .defaultScrollAnchor(.top)
        .background(p.paper.ignoresSafeArea())
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 8) {
                if screen == .chat && stream {
                    GlassEffectContainer(spacing: 10) {
                        HStack(spacing: 10) {
                            Action(text: "Stop", icon: "stop.fill") { stream = false }
                            Action(text: "Steer", icon: "arrow.turn.up.right") { steering.toggle() }
                            Spacer(minLength: 0)
                        }
                    }
                }
                Composer(hint: steering ? "What should change?" : (group ? "Message Everyone" : "Message \(person.name)"))
            }
            .frame(maxWidth: 660)
            .padding(.horizontal, 18).padding(.vertical, 8)
            .frame(maxWidth: .infinity)
            .background(p.paper.opacity(0.94))
        }
        .navigationTitle(group ? "Everyone" : person.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(group ? .visible : .hidden, for: .tabBar)
        .toolbar {
            if !group {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink(value: Screen.profile) { face(person, 24) }
                        .accessibilityLabel("\(person.name)’s profile")
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    NavigationLink("Threads", value: Screen.threads)
                    NavigationLink("Find in chat", value: Screen.search)
                } label: { Image(systemName: "ellipsis").accessibilityLabel("Chat options") }
            }
        }
    }

    @ViewBuilder var shared: some View {
        if screen == .empty {
            VStack(alignment: .leading, spacing: 14) {
                PebbleMark(concept: .pocket, colors: [p.fig, p.markClay]).frame(width: 60, height: 60)
                Text("A place for everyone").font(.largeTitle.weight(.semibold))
                Text("Ask a question or say hello. Use @name to invite a particular Marble.")
                    .foregroundStyle(p.muted)
            }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, 92)
        } else if screen == .sharedHome {
            VStack(alignment: .leading, spacing: 12) {
                Text("Good morning").font(.largeTitle.weight(.semibold)).tracking(-0.7)
                Text("Talk together, or pick someone to catch up with.").font(.subheadline).foregroundStyle(p.muted)
                let row = type.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading)) : AnyLayout(HStackLayout(spacing: 12))
                row {
                    ForEach(Person.all) { person in
                        Button { openPerson(person) } label: {
                            ViewThatFits(in: .horizontal) {
                                HStack(spacing: 8) { face(person, 30); Text(person.name).font(.subheadline.weight(.semibold)) }
                                VStack(spacing: 6) { face(person, 30); Text(person.name).font(.caption.weight(.semibold)) }
                            }
                            .padding(10).frame(maxWidth: .infinity, minHeight: 50)
                            .background(p.card, in: RoundedRectangle(cornerRadius: 14))
                            .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder(p.line) }
                        }.buttonStyle(.plain)
                    }
                }
            }
            Divider().overlay(p.line)
            UserBubble("Could we make Saturday less busy?")
            Reply(person: .ada, text: "Let’s keep just the walk and leave the afternoon open.")
        } else {
            HStack(spacing: 9) {
                Image(systemName: "at").foregroundStyle(p.fig)
                Text("Ask the room or use @name to choose who answers.").font(.footnote)
            }.foregroundStyle(p.muted).surface(p, padding: 12, radius: 14)
            stamp("Today · 9:32 AM")
            UserBubble("**@Ada** Can you help me plan a slower Saturday?")
            Reply(person: .ada, text: "Coffee first, then a walk. I’ll ask Ben for a quiet route.")
            Folded("Ada asked Ben for a quiet route.", icon: "arrow.turn.down.right", disclosure: false)
            Reply(person: .ben, text: "Try the river path before eleven. The north entrance is usually quiet.")
            Folded("Ben sent Ada the route.", icon: "arrow.turn.down.right", disclosure: false)
            NavigationLink(value: Screen.threads) { Folded("2 replies in a thread", icon: "bubble.left", disclosure: true) }
                .buttonStyle(.plain)
        }
    }

    @ViewBuilder var individual: some View {
        stamp("Today · 9:38 AM")
        UserBubble("Can you plan a Saturday walk without making the day feel packed?")
        if screen == .error {
            Reply(person: person, text: retried ? "I’m trying again." : "Let’s leave the afternoon open.")
            if !retried { ErrorCard { retried = true } }
        } else {
            Folded("How \(person.name) is thinking", icon: "text.alignleft")
            Folded("Checked your Saturday note", icon: "checkmark")
            Reply(person: person, text: stream ? "You like a slow start, so let’s keep the morning simple.\n\n**10:30 AM** · Coffee near the river, then a loop from the north entrance. I’d leave the rest of the day open…" : "Coffee at 10:30, then a loop from the north entrance. Leave the afternoon open.", working: stream)
            if stream {
                Text("\(person.name) is writing…").font(.footnote.weight(.medium)).foregroundStyle(p.wait)
                    .padding(.leading, 40)
            }
        }
    }

    func stamp(_ title: String) -> some View {
        Text(title).font(.caption.weight(.medium))
            .foregroundStyle(p.muted).frame(maxWidth: .infinity)
    }
}

struct UserBubble: View {
    let text: String
    @Environment(\.colorScheme) private var scheme
    init(_ text: String) { self.text = text }
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        Text((try? AttributedString(markdown: text)) ?? AttributedString(text))
            .font(.body).padding(.horizontal, 16).padding(.vertical, 13)
            .background(p.wash, in: UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: 20, bottomTrailingRadius: 6, topTrailingRadius: 20))
            .padding(.leading, 40)
            .frame(maxWidth: .infinity, alignment: .trailing)
            .accessibilityLabel("You said, \(text.replacingOccurrences(of: "*", with: ""))")
    }
}

struct Reply: View {
    let person: Person
    let text: String
    var working = false
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        HStack(alignment: .top, spacing: 10) {
            face(person, 30, working: working)
            VStack(alignment: .leading, spacing: 5) {
                Text(person.name).font(.subheadline.weight(.bold)).foregroundStyle(p.fig)
                Text((try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(text))
                    .font(.body).lineSpacing(3).fixedSize(horizontal: false, vertical: true)
            }
        }.accessibilityElement(children: .combine)
    }
}

struct Folded: View {
    let text: String
    let icon: String
    var disclosure = true
    @Environment(\.colorScheme) private var scheme
    @State private var expanded = false
    init(_ text: String, icon: String, disclosure: Bool = true) {
        self.text = text
        self.icon = icon
        self.disclosure = disclosure
    }
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        if disclosure && icon != "bubble.left" {
            DisclosureGroup(isExpanded: $expanded) {
                Text(icon == "checkmark" ? "Read the note about how you like to start the weekend." : "I’ll keep a late start and leave the afternoon free.")
                    .font(.footnote).padding(.top, 5)
            } label: { Label(text, systemImage: icon).font(.footnote.weight(.medium)) }
            .foregroundStyle(p.muted)
            .surface(p, padding: 10, radius: 12)
        } else {
            Label(text, systemImage: icon).font(.footnote.weight(.medium)).foregroundStyle(p.muted)
                .padding(.leading, 40)
        }
    }
}

struct Composer: View {
    let hint: String
    var demoMention = false
    @Environment(\.colorScheme) private var scheme
    @State private var draft = ""

    var body: some View {
        let p = Palette(dark: scheme == .dark)
        let matches = Person.all.filter { draft == "@" || $0.name.lowercased().hasPrefix(String(draft.dropFirst()).lowercased()) }
        VStack(alignment: .leading, spacing: 8) {
            if demoMention || (draft.hasPrefix("@") && !matches.isEmpty) {
                VStack(spacing: 0) {
                    ForEach(demoMention ? [.ada, .ben] : matches) { person in
                        Button { draft = "@\(person.name) " } label: {
                            HStack { face(person, 24); Text("@\(person.name)").font(.subheadline.weight(.semibold)); Spacer() }
                                .frame(minHeight: 44).contentShape(Rectangle())
                        }.buttonStyle(.plain)
                    }
                }
                .surface(p, padding: 8, radius: 14)
            }
            GlassEffectContainer(spacing: 9) {
                HStack(spacing: 9) {
                    HStack(spacing: 8) {
                        TextField(hint, text: $draft, prompt: Text(demoMention ? "@" : hint).foregroundStyle(p.muted), axis: .vertical)
                            .font(.body).lineLimit(1...3)
                        Button {} label: { Image(systemName: "mic").frame(width: 44, height: 44) }
                            .buttonStyle(.plain).accessibilityLabel("Dictate")
                    }
                    .padding(.leading, 16)
                    .frame(minHeight: 48)
                    .glassEffect(.regular, in: .capsule)
                    Button { draft = "" } label: {
                        Image(systemName: "arrow.up").font(.body.weight(.semibold))
                            .frame(width: 48, height: 48)
                            .foregroundStyle(draft.isEmpty && !demoMention ? p.muted : p.onFig)
                            .glassEffect(draft.isEmpty && !demoMention ? .regular : .regular.tint(p.fig).interactive(), in: .circle)
                    }.buttonStyle(.plain).accessibilityLabel("Send")
                    .disabled(draft.isEmpty && !demoMention)
                }
            }
        }
    }
}

struct ErrorCard: View {
    var retry: () -> Void = {}
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        VStack(alignment: .leading, spacing: 7) {
            Label("The reply stopped", systemImage: "exclamationmark.circle")
                .font(.headline).foregroundStyle(p.bad)
            Text("Your message is still here. Try asking again.").font(.subheadline).foregroundStyle(p.muted)
            Action(text: "Retry", icon: "arrow.clockwise", action: retry)
        }.surface(p)
    }
}

struct RequestCard: View {
    enum Kind { case approval, memory }
    let kind: Kind
    var onReview: (() -> Void)? = nil
    @State private var decision: String?
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dynamicTypeSize) private var type
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        let approval = kind == .approval
        VStack(alignment: .leading, spacing: 8) {
            Label(approval ? "Ada wants to read a file" : "Remember this?", systemImage: approval ? "hand.raised" : "bookmark")
                .font(.headline).foregroundStyle(approval ? p.wait : p.fig)
            Text(approval ? "Read Weekend plans in your workspace." : "You prefer a quiet walk before lunch.")
                .font(.subheadline).foregroundStyle(p.ink)
            if let decision {
                Text(decision).font(.footnote.weight(.medium)).foregroundStyle(p.good)
            } else {
                let layout = type.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading)) : AnyLayout(HStackLayout(spacing: 8))
                GlassEffectContainer(spacing: 8) {
                    layout {
                        Action(text: approval ? "Allow once" : "Review") {
                            if let onReview, !approval { onReview() }
                            else { decision = approval ? "Allowed once" : "Ready to review" }
                        }
                        Action(text: approval ? "Don’t allow" : "Not now") {
                            decision = approval ? "Not allowed" : "Dismissed"
                        }
                    }
                }
            }
        }.surface(p)
    }
}
