import Charts
import GlassModels
import MapKit
import SwiftUI

extension EnvironmentValues {
    /// Puts text from a reply component into the message box; the owner sends it.
    @Entry var composeText: (@MainActor (String) -> Void)? = nil
}

/// A `glass-ui` fence: drawn natively when this app knows the type, otherwise its Markdown fallback.
struct ComponentBlock: View {
    var source: String

    var body: some View {
        if let component = ReplyComponent(source: source) {
            if component.isDrawable() {
                ComponentView(component: component)
            } else {
                MarkdownView(text: component.fallbackText)
            }
        } else if source.trimmingCharacters(in: .whitespacesAndNewlines).hasSuffix("}") {
            CodeBlock(code: source, language: "json")
        } else {
            Text("Preparing view…")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("component.pending")
        }
    }
}

struct ComponentView: View {
    var component: ReplyComponent
    private var data: JSONValue { component.data }

    /// Self-contained objects (a forecast, a draft, a form) sit on a quiet outlined card; tables, charts,
    /// lists and links flow straight into the reply like the surrounding text.
    private static let carded: Set<String> = [
        "kpi", "counter", "copy", "choices", "form", "product", "business", "reservation", "video", "file",
        "writing", "weather", "sports", "currency", "flights", "jobs", "news",
    ]

    var body: some View {
        let title = data["title"]?.text
        let isCard = Self.carded.contains(component.type)
        VStack(alignment: .leading, spacing: 10) {
            if title != nil || component.example {
                HStack(alignment: .firstTextBaseline) {
                    if let title { Text(title).font(.headline).accessibilityAddTraits(.isHeader) }
                    Spacer(minLength: 8)
                    if component.example {
                        Text("Example data")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 2)
                            .overlay(Capsule().strokeBorder(Color(.separator)))
                            .accessibilityLabel("Example data, not live")
                    }
                }
            }
            content
        }
        .padding(isCard ? 14 : 0)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            if isCard {
                RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color(.systemBackground))
                RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Color(.separator), lineWidth: 0.75)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("component.\(component.type)")
    }

    @ViewBuilder private var content: some View {
        switch component.type {
        case "badges": BadgesView(data: data)
        case "icon_list": IconListView(data: data)
        case "entities": EntitiesView(data: data)
        case "kpi": KPIView(data: data)
        case "buttons": ButtonsView(data: data)
        case "counter": CounterView(data: data)
        case "copy": CopyView(data: data)
        case "choices": ChoicesView(data: data, title: data["title"]?.text)
        case "form": FormView(data: data)
        case "table": TableView(data: data)
        case "chart": ChartView(data: data)
        case "image": RemoteImage(url: data["url"]?.text, alt: data["alt"]?.text, caption: data["caption"]?.text)
        case "gallery":
            Strip(items: data.list("images")) { RemoteImage(url: $0["url"]?.text, alt: $0["alt"]?.text, caption: $0["caption"]?.text).frame(width: 220) }
        case "carousel", "hotels":
            Strip(items: data.list(component.type == "hotels" ? "results" : "cards")) { CardView(item: $0).frame(width: 240) }
        case "map": MapComponent(data: data)
        case "disclosure": DisclosureComponent(data: data)
        case "checklist": ChecklistView(data: data)
        case "progress": ProgressComponent(data: data)
        case "timeline": TimelineComponent(data: data)
        case "html": HTMLComponent(data: data)
        case "product", "business", "video", "file": CardView(item: data, kind: component.type)
        case "reservation": ReservationView(data: data)
        case "sources": SourcesView(data: data)
        case "writing": WritingView(data: data)
        case "weather": WeatherView(data: data)
        case "sports": SportsView(data: data)
        case "currency": CurrencyView(data: data)
        case "flights", "jobs", "news": ResultRows(items: data.list(component.type == "news" ? "items" : "results"), kind: component.type)
        case "flow": FlowComponent(data: data)
        default: MarkdownView(text: component.fallbackText)
        }
    }
}

// MARK: - Helpers

private func webURL(_ value: String?) -> URL? {
    guard let value, let url = URL(string: value), ["https", "http"].contains(url.scheme?.lowercased() ?? "") else { return nil }
    return url
}

private func symbol(_ name: String?, default fallback: String = "circle") -> String {
    guard let name, UIImage(systemName: name) != nil else { return fallback }
    return name
}

private func joined(_ parts: String?...) -> String? {
    let text = parts.compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
    return text.isEmpty ? nil : text
}

@MainActor private func copy(_ text: String) {
    UIPasteboard.general.string = text
    UIAccessibility.post(notification: .announcement, argument: "Copied")
}

private struct Strip<Content: View>: View {
    var items: [JSONValue]
    @ViewBuilder var row: (JSONValue) -> Content

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            LazyHStack(alignment: .top, spacing: 10) {
                ForEach(Array(items.prefix(50).enumerated()), id: \.offset) { row($0.element) }
            }
        }
    }
}

/// Wraps children onto new lines like text.
private struct Flow: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(width: proposal.width ?? .infinity, subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for (index, point) in arrange(width: bounds.width, subviews).points.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + point.x, y: bounds.minY + point.y), proposal: .unspecified)
        }
    }

    private func arrange(width: CGFloat, _ subviews: Subviews) -> (points: [CGPoint], size: CGSize) {
        var points: [CGPoint] = [], x: CGFloat = 0, y: CGFloat = 0, line: CGFloat = 0, widest: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width { x = 0; y += line + spacing; line = 0 }
            points.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            line = max(line, size.height)
            widest = max(widest, x - spacing)
        }
        return (points, CGSize(width: min(widest, width), height: y + line))
    }
}

private struct Pill: View {
    var text: String
    var tone: String?
    var outlined = false

    private var color: Color {
        switch tone {
        case "info": .blue
        case "success": .green
        case "warning": .orange
        case "danger": .red
        default: .secondary
        }
    }

    var body: some View {
        Text(text)
            .font(.caption)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .foregroundStyle(tone == nil || tone == "neutral" ? Color.primary : color)
            .background {
                if outlined {
                    Capsule().strokeBorder(Color(.separator))
                } else {
                    Capsule().fill(color.opacity(0.15))
                }
            }
    }
}

private struct Rating: View {
    var rating: Double?
    var reviews: String?

    var body: some View {
        if let rating {
            HStack(spacing: 3) {
                Image(systemName: "star.fill").foregroundStyle(.yellow).imageScale(.small)
                Text(rating, format: .number.precision(.fractionLength(1)))
                if let reviews { Text("(\(reviews))").foregroundStyle(.secondary) }
            }
            .font(.footnote)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Rated \(rating.formatted(.number.precision(.fractionLength(1)))) out of 5")
        }
    }
}

// MARK: - Components

private struct BadgesView: View {
    var data: JSONValue
    var body: some View {
        Flow {
            ForEach(Array(data.list("items").enumerated()), id: \.offset) { _, item in
                Pill(text: item["text"]?.text ?? "", tone: item["tone"]?.text, outlined: item["style"]?.text == "outlined")
            }
        }
    }
}

private struct IconListView: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(data.list("items").enumerated()), id: \.offset) { _, item in
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(item["title"]?.text ?? "")
                        if let caption = item["caption"]?.text { Text(caption).font(.footnote).foregroundStyle(.secondary) }
                    }
                } icon: {
                    Image(systemName: symbol(item["icon"]?.text)).foregroundStyle(.tint)
                }
            }
        }
    }
}

private struct EntitiesView: View {
    var data: JSONValue
    var body: some View {
        Flow {
            ForEach(Array(data.list("items").enumerated()), id: \.offset) { _, item in
                let title = item["title"]?.text ?? ""
                if let url = webURL(item["url"]?.text) {
                    Link(title, destination: url).font(.subheadline).underline(pattern: .dot)
                } else {
                    Text(title).font(.subheadline)
                }
            }
        }
    }
}

private struct KPIView: View {
    var data: JSONValue
    @State private var range: String?

    private var ranges: [(String, JSONValue)] {
        guard case .object(let object) = data["ranges"] ?? .null else { return [] }
        return object.sorted { $0.key.localizedStandardCompare($1.key) == .orderedAscending }
    }

    var body: some View {
        let shown = range.flatMap { key in ranges.first { $0.0 == key }?.1 } ?? data
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                if let icon = data["icon"]?.text { Image(systemName: symbol(icon)).foregroundStyle(.secondary) }
                Text(shown["value"]?.text ?? "").font(.title.weight(.semibold)).monospacedDigit()
                if let delta = shown["delta"]?.text {
                    let trend = shown["trend"]?.text
                    Label(delta, systemImage: trend == "down" ? "arrow.down.right" : trend == "up" ? "arrow.up.right" : "arrow.right")
                        .font(.subheadline)
                        .foregroundStyle(trend == "down" ? .red : trend == "up" ? .green : .secondary)
                }
            }
            if let subtitle = data["subtitle"]?.text { Text(subtitle).font(.footnote).foregroundStyle(.secondary) }
            if !ranges.isEmpty {
                Picker("Range", selection: $range) {
                    Text("Now").tag(String?.none)
                    ForEach(ranges, id: \.0) { Text($0.0).tag(Optional($0.0)) }
                }
                .pickerStyle(.segmented)
            }
        }
    }
}

private struct ButtonsView: View {
    var data: JSONValue
    @Environment(\.composeText) private var compose
    @Environment(\.openURL) private var openURL

    var body: some View {
        Flow(spacing: 8) {
            ForEach(Array(data.list("items").enumerated()), id: \.offset) { _, item in
                let label = item["label"]?.text ?? ""
                let action = item["action"] ?? .null
                let button = Button(label) {
                    if let url = webURL(action["url"]?.text) { openURL(url) }
                    else if let text = action["copy"]?.text { copy(text) }
                    else { compose?(action["send"]?.text ?? label) }
                }
                switch item["style"]?.text {
                case "primary": button.buttonStyle(.borderedProminent)
                case "plain": button.buttonStyle(.borderless)
                default: button.buttonStyle(.bordered)
                }
            }
        }
    }
}

private struct CounterView: View {
    var data: JSONValue
    @State private var value: Double?

    var body: some View {
        let step = data["step"]?.number ?? 1
        let low = data["min"]?.number ?? -Double.greatestFiniteMagnitude
        let high = data["max"]?.number ?? Double.greatestFiniteMagnitude
        Stepper(value: Binding(get: { value ?? data["value"]?.number ?? 0 }, set: { value = $0 }), in: low...max(low, high), step: step) {
            HStack {
                Text(data["label"]?.text ?? "")
                Spacer()
                Text(JSONValue.number(value ?? data["value"]?.number ?? 0).text ?? "").monospacedDigit()
            }
        }
    }
}

private struct CopyView: View {
    var data: JSONValue
    var body: some View {
        let text = data["text"]?.text ?? ""
        HStack {
            Text(text).font(.system(.callout, design: .monospaced)).textSelection(.enabled).lineLimit(3)
            Spacer(minLength: 8)
            Button(data["label"]?.text ?? "Copy", systemImage: "doc.on.doc") { copy(text) }
                .buttonStyle(.bordered)
        }
    }
}

private struct ChoicesView: View {
    var data: JSONValue
    var title: String?
    @State private var picked: [String] = []
    @Environment(\.composeText) private var compose

    var body: some View {
        let multiple = data["multiple"]?.boolValue ?? false
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(data.list("options").enumerated()), id: \.offset) { _, option in
                let value = option["title"]?.text ?? option["id"]?.text ?? ""
                let on = picked.contains(value)
                Button {
                    if on { picked.removeAll { $0 == value } } else { picked = multiple ? picked + [value] : [value] }
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(value).foregroundStyle(.primary)
                            if let subtitle = option["subtitle"]?.text { Text(subtitle).font(.footnote).foregroundStyle(.secondary) }
                        }
                        Spacer()
                        Image(systemName: on ? "checkmark.circle.fill" : "circle").foregroundStyle(on ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .contentShape(.rect)
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(on ? AnyShapeStyle(.tint) : AnyShapeStyle(Color(.separator)), lineWidth: on ? 1.5 : 0.75))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
            Button(data["submit"]?.text ?? "Use Selection") {
                compose?(ReplyComponent.answer(title: title, [("Selected", picked.joined(separator: ", "))]))
            }
            .buttonStyle(.borderedProminent)
            .disabled(picked.isEmpty)
        }
    }
}

private struct FormView: View {
    var data: JSONValue
    @State private var values: [String: String] = [:]
    @State private var sets: [String: Set<String>] = [:]
    @State private var dates: [String: Date] = [:]
    @Environment(\.composeText) private var compose

    private func options(_ field: JSONValue) -> [String] {
        field.list("options").compactMap { $0.text ?? $0["label"]?.text ?? $0["title"]?.text }
    }

    var body: some View {
        let fields = data.list("fields")
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(fields.enumerated()), id: \.offset) { _, field in
                let id = field["id"]?.text ?? field["label"]?.text ?? ""
                let label = field["label"]?.text ?? id
                let text = Binding(get: { values[id] ?? "" }, set: { values[id] = $0 })
                switch field["kind"]?.text {
                case "select", "radio":
                    LabeledContent(label) {
                        Picker(label, selection: text) {
                            Text("Choose").tag("")
                            ForEach(options(field), id: \.self) { Text($0).tag($0) }
                        }
                        .pickerStyle(.menu)
                        .labelsHidden()
                    }
                case "checkbox":
                    VStack(alignment: .leading, spacing: 4) {
                        Text(label).font(.subheadline)
                        ForEach(options(field), id: \.self) { option in
                            Toggle(option, isOn: Binding(get: { sets[id, default: []].contains(option) },
                                                         set: { on in if on { sets[id, default: []].insert(option) } else { sets[id]?.remove(option) } }))
                        }
                    }
                case "slider":
                    let low = field["min"]?.number ?? 0, high = max(low + 1, field["max"]?.number ?? 100)
                    VStack(alignment: .leading) {
                        Text("\(label): \(values[id] ?? JSONValue.number(low).text ?? "")").font(.subheadline)
                        Slider(value: Binding(get: { Double(values[id] ?? "") ?? low }, set: { values[id] = JSONValue.number($0).text }),
                               in: low...high, step: field["step"]?.number ?? 1)
                            .accessibilityLabel(label)
                    }
                case "date":
                    DatePicker(label, selection: Binding(get: { dates[id] ?? .now }, set: { dates[id] = $0; values[id] = $0.formatted(date: .abbreviated, time: .omitted) }),
                               displayedComponents: .date)
                default:
                    TextField(label, text: text, prompt: Text(field["placeholder"]?.text ?? label),
                              axis: field["kind"]?.text == "textarea" ? .vertical : .horizontal)
                        .textFieldStyle(.roundedBorder)
                        .keyboardType(field["kind"]?.text == "email" ? .emailAddress : .default)
                        .textInputAutocapitalization(field["kind"]?.text == "email" ? .never : .sentences)
                }
            }
            Button(data["submit"]?.text ?? "Add Answers to Message") {
                let entries: [(String, String)] = fields.compactMap { field in
                    let id = field["id"]?.text ?? field["label"]?.text ?? ""
                    let value = values[id] ?? sets[id].map { $0.sorted().joined(separator: ", ") } ?? ""
                    return value.isEmpty ? nil : (field["label"]?.text ?? id, value)
                }
                compose?(ReplyComponent.answer(title: data["title"]?.text, entries))
            }
            .buttonStyle(.borderedProminent)
        }
    }
}

private struct TableView: View {
    var data: JSONValue
    @State private var filter = ""

    var body: some View {
        let columns: [(key: String, label: String)] = data.list("columns").map {
            let key = $0["key"]?.text ?? $0.text ?? ""
            return (key, $0["label"]?.text ?? key)
        }
        let rows = data.list("rows").prefix(500)
        let filterKey = data["filter"]?.text.flatMap { key in columns.first { $0.key == key } }
        let shown = rows.filter { filter.isEmpty || $0[filterKey?.key ?? ""]?.text == filter }
        VStack(alignment: .leading, spacing: 8) {
            if let filterKey {
                let choices = Array(NSOrderedSet(array: rows.compactMap { $0[filterKey.key]?.text })) as? [String] ?? []
                Picker(filterKey.label, selection: $filter) {
                    Text("All").tag("")
                    ForEach(choices.prefix(30), id: \.self) { Text($0).tag($0) }
                }
                .pickerStyle(.menu)
            }
            DataTable(header: columns.map(\.label),
                      rows: shown.map { row in columns.map { row[$0.key]?.text ?? "" } }, markdown: false)
        }
    }
}

struct ChartView: View {
    var data: JSONValue
    @State private var angle: Double?
    @State private var numericX: Double?
    @State private var categoryX: String?

    struct Point: Identifiable {
        var id: Int
        var series: String
        var x: String
        var xValue: Double?
        var y: Double
    }

    var body: some View {
        let kind = data["kind"]?.text ?? "line"
        var index = 0
        let points: [Point] = data.list("series").flatMap { series in
            series.list("points").compactMap { point -> Point? in
                guard let y = point["y"]?.number, y.isFinite, kind != "pie" || y > 0 else { return nil }
                index += 1
                return Point(id: index, series: series["name"]?.text ?? "", x: point["x"]?.text ?? "", xValue: point["x"]?.number, y: y)
            }
        }
        let numeric = !points.isEmpty && points.allSatisfy { $0.xValue != nil } && kind != "bar"
        let selected: [Point] = switch kind {
        case "pie": angle.flatMap { Self.slice(at: $0, in: points) }.map { [$0] } ?? []
        case _ where numeric: numericX.flatMap { value in
            points.min { abs(($0.xValue ?? 0) - value) < abs(($1.xValue ?? 0) - value) }
        }.map { closest in points.filter { $0.xValue == closest.xValue } } ?? []
        default: points.filter { $0.x == categoryX }
        }
        let chart = Chart(points) { point in
                switch kind {
                case "pie":
                    SectorMark(angle: .value("Value", point.y), innerRadius: .ratio(0.5), angularInset: 1)
                        .foregroundStyle(by: .value("Label", point.x))
                        .opacity(selected.isEmpty || selected.first?.id == point.id ? 1 : 0.38)
                case "bar":
                    BarMark(x: .value(data["x_label"]?.text ?? "X", point.x), y: .value(data["y_label"]?.text ?? "Y", point.y))
                        .foregroundStyle(by: .value("Series", point.series))
                        .position(by: .value("Series", point.series))
                case "scatter":
                    if numeric { PointMark(x: .value("X", point.xValue ?? 0), y: .value("Y", point.y)).foregroundStyle(by: .value("Series", point.series)) }
                    else { PointMark(x: .value("X", point.x), y: .value("Y", point.y)).foregroundStyle(by: .value("Series", point.series)) }
                case "area":
                    if numeric { AreaMark(x: .value("X", point.xValue ?? 0), y: .value("Y", point.y)).foregroundStyle(by: .value("Series", point.series)) }
                    else { AreaMark(x: .value("X", point.x), y: .value("Y", point.y)).foregroundStyle(by: .value("Series", point.series)) }
                default:
                    if numeric { LineMark(x: .value("X", point.xValue ?? 0), y: .value("Y", point.y)).foregroundStyle(by: .value("Series", point.series)) }
                    else { LineMark(x: .value("X", point.x), y: .value("Y", point.y)).foregroundStyle(by: .value("Series", point.series)) }
                }
        }
        .chartLegend(Set(points.map(\.series)).count > 1 || kind == "pie" ? .visible : .hidden)
        .accessibilityLabel(data["title"]?.text ?? "Chart")
        .accessibilityIdentifier("component.chart.plot")
        VStack(alignment: .leading, spacing: Design.small) {
            Group {
                if kind == "pie" {
                    chart.chartAngleSelection(value: retaining($angle))
                        .chartGesture { proxy in
                            DragGesture(minimumDistance: 0).onChanged {
                                proxy.selectAngleValue(at: proxy.angle(at: $0.location))
                            }
                        }
                } else if numeric {
                    chart.chartXSelection(value: retaining($numericX))
                } else {
                    chart.chartXSelection(value: retaining($categoryX))
                }
            }
            .frame(height: kind == "pie" ? 220 : 200)
            ForEach(selected) { point in
                let label = point.series.isEmpty ? point.x : "\(point.x) · \(point.series)"
                let value = point.y.formatted(.number.precision(.fractionLength(0...340)))
                LabeledContent(label, value: value)
                    .font(.caption.monospacedDigit())
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(label)
                    .accessibilityValue(value)
                    .accessibilityIdentifier("component.chart.value")
            }
            if let caption = joined(data["x_label"]?.text, data["y_label"]?.text) {
                Text(caption).font(.caption).foregroundStyle(.secondary)
            }
        }
        .onChange(of: data) { _, _ in
            angle = nil
            numericX = nil
            categoryX = nil
        }
    }

    private func retaining<Value>(_ binding: Binding<Value?>) -> Binding<Value?> {
        Binding(get: { binding.wrappedValue }, set: { value in
            if let value { binding.wrappedValue = value }
        })
    }

    static func slice(at angle: Double, in points: [Point]) -> Point? {
        guard angle.isFinite, angle >= 0 else { return nil }
        var upper = 0.0
        for point in points {
            upper += point.y
            if angle < upper { return point }
        }
        return angle == upper ? points.last : nil
    }
}

private struct RemoteImage: View {
    var url: String?
    var alt: String?
    var caption: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let url = url.flatMap(URL.init(string:)), url.scheme == "https" {
                RemoteMarkdownImage(url: url, alt: alt ?? "").id(url)
            } else {
                Label(alt ?? "Image unavailable", systemImage: "photo").foregroundStyle(.secondary)
            }
            if let caption { Text(caption).font(.caption).foregroundStyle(.secondary) }
        }
    }
}

private struct CardView: View {
    var item: JSONValue
    var kind = "card"

    var body: some View {
        let title = item["title"]?.text ?? item["name"]?.text ?? ""
        VStack(alignment: .leading, spacing: 4) {
            if let image = item["image"]?.text ?? item["poster"]?.text {
                RemoteImage(url: image, alt: title).frame(maxHeight: 160)
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                if kind == "video" { Image(systemName: "play.rectangle").foregroundStyle(.secondary) }
                if kind == "file" { Image(systemName: "doc").foregroundStyle(.secondary) }
                if let url = webURL(item["url"]?.text) { Link(title, destination: url).font(.headline) } else { Text(title).font(.headline) }
            }
            let detail: String? = switch kind {
            case "product": joined(item["price"]?.text, item["merchant"]?.text)
            case "business": joined(item["category"]?.text, item["price_level"]?.text)
            case "video": item["duration"]?.text
            case "file": joined(item["kind"]?.text, item["size"]?.text, item["path"]?.text)
            default: joined(item["subtitle"]?.text, item["area"]?.text, item["price"]?.text)
            }
            if let detail { Text(detail).font(.subheadline).foregroundStyle(.secondary) }
            if let body = item["body"]?.text { Text(body).font(.callout) }
            Rating(rating: item["rating"]?.number, reviews: item["reviews"]?.text)
            if kind == "business" {
                ForEach([("mappin", item["address"]?.text), ("clock", item["hours"]?.text), ("phone", item["phone"]?.text)], id: \.0) { icon, value in
                    if let value { Label(value, systemImage: icon).font(.footnote) }
                }
            }
            if kind == "product", !item.list("badges").isEmpty {
                Flow { ForEach(Array(item.list("badges").enumerated()), id: \.offset) { Pill(text: $0.element.text ?? "", tone: nil) } }
            }
            if let excerpt = item["excerpt"]?.text {
                Text(excerpt).font(.system(.caption, design: .monospaced)).lineLimit(8).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

private struct MapComponent: View {
    var data: JSONValue

    var body: some View {
        let pins = data.list("pins").compactMap { pin -> (String, String, CLLocationCoordinate2D)? in
            guard let lat = pin["lat"]?.number, let lon = pin["lon"]?.number, abs(lat) <= 90, abs(lon) <= 180 else { return nil }
            return (pin["label"]?.text ?? "", symbol(pin["icon"]?.text, default: "mappin"), CLLocationCoordinate2D(latitude: lat, longitude: lon))
        }
        VStack(alignment: .leading, spacing: 6) {
            Map(initialPosition: .automatic, interactionModes: [.pan, .zoom]) {
                ForEach(Array(pins.enumerated()), id: \.offset) { _, pin in
                    Marker(pin.0, systemImage: pin.1, coordinate: pin.2)
                }
            }
            .frame(height: 220)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            ForEach(Array(pins.enumerated()), id: \.offset) { _, pin in
                Text(pin.0).font(.footnote)
            }
        }
    }
}

private struct DisclosureComponent: View {
    var data: JSONValue
    @State private var open = false

    var body: some View {
        let title = data["title"]?.text ?? ""
        let body = data["body"]?.text ?? ""
        if data["style"]?.text == "popover" || data["style"]?.text == "tooltip" {
            Button(title, systemImage: "info.circle") { open = true }
                .buttonStyle(.borderless)
                .popover(isPresented: $open) {
                    MarkdownView(text: body).padding().frame(idealWidth: 300).presentationCompactAdaptation(.popover)
                }
        } else {
            DisclosureGroup(title, isExpanded: $open) { MarkdownView(text: body).padding(.top, 4) }
        }
    }
}

private struct ChecklistView: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(data.list("items").enumerated()), id: \.offset) { _, item in
                let done = item["done"]?.boolValue ?? false
                Label {
                    Text(item["text"]?.text ?? "").strikethrough(done).foregroundStyle(done ? .secondary : .primary)
                } icon: {
                    Image(systemName: done ? "checkmark.circle.fill" : "circle").foregroundStyle(done ? AnyShapeStyle(.green) : AnyShapeStyle(.tertiary))
                }
                .accessibilityValue(done ? "Done" : "Not done")
            }
        }
    }
}

private struct ProgressComponent: View {
    var data: JSONValue
    var body: some View {
        let value = min(1, max(0, data["value"]?.number ?? 0))
        let label = data["label"]?.text ?? ""
        if data["style"]?.text == "ring" {
            HStack(spacing: 12) {
                Gauge(value: value) { Text(label) } currentValueLabel: { Text(value, format: .percent.precision(.fractionLength(0))) }
                    .gaugeStyle(.accessoryCircularCapacity)
                    .tint(.accentColor)
                VStack(alignment: .leading) {
                    Text(label)
                    if let caption = data["caption"]?.text { Text(caption).font(.footnote).foregroundStyle(.secondary) }
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 4) {
                ProgressView(value: value) {
                    HStack { Text(label); Spacer(); Text(value, format: .percent.precision(.fractionLength(0))).monospacedDigit() }
                }
                if let caption = data["caption"]?.text { Text(caption).font(.footnote).foregroundStyle(.secondary) }
            }
        }
    }
}

private struct TimelineComponent: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(data.list("steps").enumerated()), id: \.offset) { _, step in
                let status = step["status"]?.text ?? "pending"
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    Image(systemName: status == "done" ? "checkmark.circle.fill" : status == "current" ? "circle.inset.filled" : "circle")
                        .foregroundStyle(status == "pending" ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.tint))
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Text(step["title"]?.text ?? "").fontWeight(status == "current" ? .semibold : .regular)
                            if let time = step["time"]?.text { Text(time).font(.footnote).foregroundStyle(.secondary) }
                        }
                        if let detail = step["detail"]?.text { Text(detail).font(.footnote).foregroundStyle(.secondary) }
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityValue(status == "done" ? "Done" : status == "current" ? "In progress" : "Not started")
            }
        }
    }
}

private struct HTMLComponent: View {
    var data: JSONValue
    @Environment(\.previewHTML) private var previewHTML

    var body: some View {
        Button("Open Interactive View", systemImage: "play.rectangle") { previewHTML?(data["html"]?.text ?? "") }
            .buttonStyle(.bordered)
            .disabled(previewHTML == nil)
    }
}

private struct ReservationView: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(data["venue"]?.text ?? "").font(.headline)
            let party = data["party_size"]?.text.map { "\($0) people" }
            if let detail = joined(data["date"]?.text, party) { Text(detail).font(.subheadline).foregroundStyle(.secondary) }
            Flow(spacing: 8) {
                ForEach(Array(data.list("slots").enumerated()), id: \.offset) { _, slot in
                    let time = slot["time"]?.text ?? ""
                    if let url = webURL(slot["url"]?.text) {
                        Link(time, destination: url).buttonStyle(.bordered)
                    } else {
                        Text(time).padding(.horizontal, 10).padding(.vertical, 6).overlay(Capsule().strokeBorder(Color(.separator)))
                    }
                }
            }
        }
    }
}

private struct SourcesView: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(data.list("items").enumerated()), id: \.offset) { index, item in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text("\(index + 1)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: 2) {
                        let title = item["title"]?.text ?? ""
                        if let url = webURL(item["url"]?.text) { Link(title, destination: url) } else { Text(title) }
                        if let site = item["site"]?.text { Text(site).font(.caption).foregroundStyle(.secondary) }
                        if let snippet = item["snippet"]?.text { Text(snippet).font(.footnote).foregroundStyle(.secondary) }
                    }
                }
            }
        }
    }
}

private struct WritingView: View {
    var data: JSONValue
    @Environment(\.openURL) private var openURL

    var body: some View {
        let body = data["body"]?.text ?? ""
        let subject = data["subject"]?.text
        let full = [subject.map { "Subject: \($0)" }, body].compactMap { $0 }.joined(separator: "\n\n")
        VStack(alignment: .leading, spacing: 8) {
            ForEach([("To", data["to"]?.text), ("Subject", subject), ("For", data["platform"]?.text)], id: \.0) { label, value in
                if let value {
                    HStack(alignment: .firstTextBaseline) {
                        Text(label).foregroundStyle(.secondary).frame(width: 64, alignment: .leading)
                        Text(value)
                    }
                    .font(.subheadline)
                }
            }
            Divider()
            Text(body).textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack {
                Button("Copy", systemImage: "doc.on.doc") { copy(full) }
                ShareLink(item: full) { Label("Share", systemImage: "square.and.arrow.up") }
                if data["kind"]?.text == "email" {
                    Button("Open in Mail", systemImage: "envelope") {
                        var parts = URLComponents()
                        parts.scheme = "mailto"
                        parts.path = data["to"]?.text ?? ""
                        parts.queryItems = [URLQueryItem(name: "subject", value: subject ?? ""), URLQueryItem(name: "body", value: body)]
                        if let url = parts.url { openURL(url) }
                    }
                }
            }
            .buttonStyle(.bordered)
            .labelStyle(.titleAndIcon)
            .font(.subheadline)
        }
    }
}

private struct WeatherView: View {
    var data: JSONValue
    var body: some View {
        let current = data["current"] ?? .null
        let unit = data["unit"]?.text == "F" ? "°F" : "°C"
        VStack(alignment: .leading, spacing: 8) {
            if let location = data["location"]?.text, data["title"] == nil { Text(location).font(.headline) }
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Image(systemName: symbol(current["icon"]?.text, default: "thermometer.medium")).symbolRenderingMode(.multicolor)
                Text("\(current["temp"]?.text ?? "–")\(unit)").font(.largeTitle.weight(.semibold)).monospacedDigit()
                Text(current["condition"]?.text ?? "").foregroundStyle(.secondary)
            }
            let high = current["high"]?.text.map { "High \($0)°" }, low = current["low"]?.text.map { "Low \($0)°" }
            if let line = joined(high, low, current["humidity"]?.text.map { "Humidity \($0)" }, current["wind"]?.text.map { "Wind \($0)" }) {
                Text(line).font(.footnote).foregroundStyle(.secondary)
            }
            let forecast = data.list("forecast")
            if !forecast.isEmpty {
                Divider()
                ForEach(Array(forecast.prefix(10).enumerated()), id: \.offset) { _, day in
                    HStack {
                        Text(day["day"]?.text ?? "").frame(width: 56, alignment: .leading)
                        Image(systemName: symbol(day["icon"]?.text, default: "cloud")).symbolRenderingMode(.multicolor).frame(width: 28)
                        Text(day["condition"]?.text ?? "").foregroundStyle(.secondary)
                        Spacer()
                        Text("\(day["high"]?.text ?? "–")° / \(day["low"]?.text ?? "–")°").monospacedDigit()
                    }
                    .font(.subheadline)
                    .accessibilityElement(children: .combine)
                }
            }
            if let asOf = data["as_of"]?.text { Text("As of \(asOf)").font(.caption).foregroundStyle(.secondary) }
        }
    }
}

private struct SportsView: View {
    var data: JSONValue
    var body: some View {
        let team = { (side: JSONValue?) in
            VStack(spacing: 2) {
                Text(side?["name"]?.text ?? "").font(.subheadline).multilineTextAlignment(.center)
                Text(side?["score"]?.text ?? "–").font(.title.weight(.semibold)).monospacedDigit()
            }
            .frame(maxWidth: .infinity)
        }
        VStack(spacing: 6) {
            HStack {
                team(data["home"])
                Text(data["status"]?.text ?? "").font(.caption).foregroundStyle(.secondary)
                team(data["away"])
            }
            if let detail = joined(data["league"]?.text, data["time"]?.text, data["venue"]?.text) {
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

private struct CurrencyView: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("\(data["converted"]?.text ?? "") \(data["to"]?.text ?? "")").font(.title.weight(.semibold)).monospacedDigit()
            let asOf = data["as_of"]?.text.map { "as of \($0)" }
            Text(joined("\(data["amount"]?.text ?? "") \(data["from"]?.text ?? "") at \(data["rate"]?.text ?? "")", asOf) ?? "")
                .font(.footnote).foregroundStyle(.secondary)
        }
    }
}

private struct ResultRows: View {
    var items: [JSONValue]
    var kind: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(items.prefix(50).enumerated()), id: \.offset) { index, item in
                if index > 0 { Divider().padding(.vertical, 8) }
                VStack(alignment: .leading, spacing: 2) {
                    let title = kind == "flights" ? "\(item["depart"]?.text ?? "") → \(item["arrive"]?.text ?? "")" : item["title"]?.text ?? ""
                    HStack(alignment: .firstTextBaseline) {
                        if let url = webURL(item["url"]?.text) { Link(title, destination: url).font(.headline) } else { Text(title).font(.headline) }
                        Spacer()
                        if let price = item["price"]?.text { Text(price).font(.subheadline.weight(.semibold)) }
                    }
                    let stops = item["stops"]?.number.map { $0 == 0 ? "Nonstop" : "\(Int($0)) stop\($0 > 1 ? "s" : "")" }
                    let detail: String? = switch kind {
                    case "flights": joined(item["airline"]?.text, item["duration"]?.text, stops)
                    case "jobs": joined(item["company"]?.text, item["location"]?.text)
                    default: joined(item["source"]?.text, item["published"]?.text)
                    }
                    if let detail { Text(detail).font(.subheadline).foregroundStyle(.secondary) }
                    if kind == "jobs", let extra = joined(item["salary"]?.text, item["posted"]?.text) {
                        Text(extra).font(.footnote).foregroundStyle(.secondary)
                    }
                    if let summary = item["summary"]?.text { Text(summary).font(.footnote) }
                }
                .accessibilityElement(children: .combine)
            }
        }
    }
}

private struct FlowComponent: View {
    var data: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            let steps = data.list("steps")
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: symbol(step["icon"]?.text, default: "\(min(index + 1, 50)).circle"))
                        .font(.title3)
                        .foregroundStyle(.tint)
                        .frame(width: 28)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(step["title"]?.text ?? "").font(.subheadline.weight(.semibold))
                        if let detail = step["detail"]?.text { Text(detail).font(.footnote).foregroundStyle(.secondary) }
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Step \(index + 1): \(step["title"]?.text ?? "")")
                if index < steps.count - 1 {
                    Image(systemName: "arrow.down").font(.caption).foregroundStyle(.tertiary).frame(width: 28).padding(.vertical, 4)
                        .accessibilityHidden(true)
                }
            }
        }
    }
}
