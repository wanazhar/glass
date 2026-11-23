# Reply components

[Documentation index](INDEX.md) · [Remote workspace](connect.md)

Glass clients can draw some replies as native components: tables, charts,
forms, choices, cards, timelines, maps, drafts, and live-data views such as
weather or flights. The same conversation stays readable in every client,
including the terminal, because each component carries a Markdown fallback.

## Block format

A component is a fenced block with the info string `glass-ui` and one JSON
object:

````text
```glass-ui
{"type":"table","v":1,"fallback":"| Name | Status |\n| --- | --- |\n| api | up |","data":{"columns":[{"key":"name","label":"Name"}],"rows":[{"name":"api"}]}}
```
````

| Field | Meaning |
| --- | --- |
| `type` | One of the 35 schema v1 types, listed in `TYPES` in `crates/glass-tui/src/components.rs` |
| `v` | Schema version, currently `1` |
| `fallback` | Required Markdown that stands on its own |
| `example` | `true` when the data is made up; clients label it as example data |
| `data` | The type's structured fields |

A block is capped at 128 KB. Core saves each assistant reply with every block
normalized: it adds `v`, and it derives a fallback when the model left one out.
A table becomes a Markdown table, a chart becomes a short data summary, cards
become bullet links, and a form becomes a numbered question list. Broken or
unclosed blocks are saved as written.

## Capability negotiation

Every `prompt` and `steer` message carries the sending client's render
capabilities:

```json
{"t":"prompt","text":"Compare the plans","client":{"name":"ios","schema":1,"components":["table","chart"]}}
```

Core decides for each turn, not for the whole conversation:

- It names the client in the system prompt for that turn, for example "the user
  is on the Glass iOS app".
- When the client lists component types on the same schema version, Core adds
  the usage rules and the catalog for only those types.
- The terminal sends an empty list, so terminal turns get no component
  instructions.
- Unknown types are dropped. A client on another schema version is treated as
  drawing none.
- Queued follow-ups keep the capabilities of the client that queued them.
- Connect lists `reply-components` in `/api/status` capabilities, so a client
  can tell when its host is too old to pass capabilities to the model.

The instructions tell the model to use tools for live data, never to invent
it, and to mark made-up values with `"example":true`. Writing drafts are never
sent automatically; the owner copies them, shares them, or opens them in Mail.
Buttons, choices, and forms only put text into the message box.

## Continuing on another client

Stored replies keep both the structured data and the fallback. The model
always sees both in its history, whichever client is active now.

| Conversation moves | Result |
| --- | --- |
| Terminal to iPhone | Earlier plain replies stay plain; new replies may use components |
| iPhone to terminal | Component replies show their fallback |
| Back to iPhone or web | The same stored replies draw as components again |

Clients show the fallback for any type they don't support, a newer schema
version, or a block they can't read.
