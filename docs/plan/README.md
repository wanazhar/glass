# Glass delivery plans

## Current source line: Glass 0.3.14

Status: Current 0.3.14 source behavior for the 2026-08-29 release record.
The `0.3.13` release notes and migration guide are historical records for the
earlier tagged release; they are not current-source instructions. The
[documentation index](../INDEX.md#glass-documentation) is the navigation map
for this source line.

The [0.3.14 release notes](../releases/0.3.14.md) and
[0.3.14 migration guide](../migration/0.3.14.md) route the current release
contract. The [0.3.14 release evidence](../release-evidence.md#0.3.14-release-evidence)
holds the exact-source records and closed publication evidence. The
historical [0.3.13 release notes](../releases/0.3.13.md) and
[0.3.13 migration guide](../migration/0.3.13.md) retain their original
version claims.

| Current source area | Status and reference |
|---|---|
| TUI onboarding and development-suite launch | 0.3.14; [Development TUI](../architecture/development-tui.md) and [Development Runtime](../development-runtime.md) |
| Editor source/diff rendering, soft-wrap, cursor synchronization, and review state | 0.3.14; [Development TUI](../architecture/development-tui.md) and [Development Runtime](../development-runtime.md) |
| Actor-attributed editor collaboration | 0.3.14; [Development Runtime](../development-runtime.md) and [MCP tool catalog](../mcp-tools.md) |
| Kitty/live browser presentation | 0.3.14; [Mobile and remote](../mobile-remote.md) and [browser connection](../architecture/browser-connection.md) |
| Pi runtime and external harness workflow | Current checkout with Pi SDK 0.84.4; [Native Pi SDK runtime](../pi-sdk-runtime.md), [Development Runtime](../development-runtime.md), and [CLI](../cli.md) |

## Active plan: Glass native browser engine (issue #40)

Status: the bounded foundation through `native-engine-234` is complete locally;
the browser-complete expansion has completed
`native-engine-browser-667`, following completed `native-engine-browser-666`,
following completed `native-engine-browser-665`,
following completed `native-engine-browser-664`,
following completed `native-engine-browser-663`,
following completed `native-engine-browser-662`,
following completed `native-engine-browser-661`,
following completed `native-engine-browser-660`, following completed
`native-engine-browser-659`, following completed
`native-engine-browser-658`, following completed
`native-engine-browser-657`, following completed
`native-engine-browser-656`, following completed
`native-engine-browser-655`, following completed
`native-engine-browser-654`, following completed
`native-engine-browser-653`, following completed
`native-engine-browser-652`, following completed
`native-engine-browser-651`, following completed
`native-engine-browser-650`, following completed
`native-engine-browser-649`, following completed
`native-engine-browser-648`, following completed
`native-engine-browser-647`, following completed
`native-engine-browser-646`, following completed
`native-engine-browser-645`, following completed
`native-engine-browser-644`, following completed
`native-engine-browser-643`, following completed
`native-engine-browser-642`, following completed
`native-engine-browser-641`, following completed
`native-engine-browser-640`, following completed
`native-engine-browser-639`, following completed
`native-engine-browser-638`, following completed
`native-engine-browser-637`, following completed
`native-engine-browser-636`, following completed
`native-engine-browser-635`, following completed
`native-engine-browser-634`, following completed

The completed bounded pseudo-class selector follow-up is
[`native-engine-browser-640`](tasks/native-engine-browser-640.md): native
compound selectors now parse bounded structural pseudo-classes (`:root`,
`:first-child`, `:last-child`, `:only-child`, `:empty`) plus bounded state and
link forms (`:checked`, `:disabled`, `:enabled`, `:required`, `:optional`,
`:link`, and `:any-link`). Document-aware matching feeds both CSS action
locators and stylesheet cascade, while unsupported functional, dynamic, and
pseudo-element forms remain fail-closed. Full selector grammar, child/sibling
combinators, pseudo-elements, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.

`native-engine-browser-633`, following completed
`native-engine-browser-632`, following completed
`native-engine-browser-631`, following completed
`native-engine-browser-630`, following completed
`native-engine-browser-629`, following completed
`native-engine-browser-628`, following completed
`native-engine-browser-627`, following completed
`native-engine-browser-626`, following completed
`native-engine-browser-625`, following completed
`native-engine-browser-624`, following completed
`native-engine-browser-623`, following completed
`native-engine-browser-622`, following completed
`native-engine-browser-621`, following completed
`native-engine-browser-620`, following completed
`native-engine-browser-619`, following completed
`native-engine-browser-618`, following completed
`native-engine-browser-617`, following completed
`native-engine-browser-616`, following completed
`native-engine-browser-615`, following completed
`native-engine-browser-614`, following completed
`native-engine-browser-613`, following completed
`native-engine-browser-612`, following completed
`native-engine-browser-611`, following completed
`native-engine-browser-610`, following completed
`native-engine-browser-609`, following completed
`native-engine-browser-608`, following completed
`native-engine-browser-607`, following completed
`native-engine-browser-606`, following completed
`native-engine-browser-605`, following completed
`native-engine-browser-604`, following completed
`native-engine-browser-603`, following completed
`native-engine-browser-602`, following completed
`native-engine-browser-601`, following completed
`native-engine-browser-600`, following completed
`native-engine-browser-599`, following completed
`native-engine-browser-598`, following completed
`native-engine-browser-597`, following completed
`native-engine-browser-596`, following completed
`native-engine-browser-595`, following completed
`native-engine-browser-594`, following completed
`native-engine-browser-593`, following completed
`native-engine-browser-592`, following completed
`native-engine-browser-591`, following completed
`native-engine-browser-590`, following completed
`native-engine-browser-589`, following completed
`native-engine-browser-588`, following completed
`native-engine-browser-587`, following completed
`native-engine-browser-586`, following completed
`native-engine-browser-585`, following completed
`native-engine-browser-584`, following completed
`native-engine-browser-583`, following completed
`native-engine-browser-582`, following completed
`native-engine-browser-581`, following completed
`native-engine-browser-580`, following completed
`native-engine-browser-579`, following completed
`native-engine-browser-578`, following completed
`native-engine-browser-577`, following completed
`native-engine-browser-576`, following completed
`native-engine-browser-575`, following completed
`native-engine-browser-574`, following completed
`native-engine-browser-573`, following completed
`native-engine-browser-572`, following completed
`native-engine-browser-571`, following completed
`native-engine-browser-570`, following completed
`native-engine-browser-569`, following completed
`native-engine-browser-568`, following completed
`native-engine-browser-567`, following completed
`native-engine-browser-566`, following completed
`native-engine-browser-565`, following completed
`native-engine-browser-564`, following completed
`native-engine-browser-563`, following completed
`native-engine-browser-562`, following completed
`native-engine-browser-561`, following completed
`native-engine-browser-560`, following completed
`native-engine-browser-559`, following completed
`native-engine-browser-558`, following completed
`native-engine-browser-557`, following completed
`native-engine-browser-556`, following completed
`native-engine-browser-555`, following completed
`native-engine-browser-554`, following completed
`native-engine-browser-553`, following completed
`native-engine-browser-552`, following completed
`native-engine-browser-551`, following completed
`native-engine-browser-550`, following completed
`native-engine-browser-549`, following completed
`native-engine-browser-548`, following completed
`native-engine-browser-547`, following completed
`native-engine-browser-546`, following completed
`native-engine-browser-545`, following completed
`native-engine-browser-544`, following completed
`native-engine-browser-543`, following completed
`native-engine-browser-542`, following completed
`native-engine-browser-541`, following completed
`native-engine-browser-540`, following completed
`native-engine-browser-539`, following completed
`native-engine-browser-538`, following completed
`native-engine-browser-537`, following completed
`native-engine-browser-536`, following completed
`native-engine-browser-535`, following completed
`native-engine-browser-534`, following completed
`native-engine-browser-533`, following completed
`native-engine-browser-532`, following completed
`native-engine-browser-531`, following completed
`native-engine-browser-530`, following completed
`native-engine-browser-528`, following completed
`native-engine-browser-527`, following completed
`native-engine-browser-526`, following completed
`native-engine-browser-525`, following completed
`native-engine-browser-524`, following completed
`native-engine-browser-523`, following completed
`native-engine-browser-522`, following completed
`native-engine-browser-521`, following completed
`native-engine-browser-520`, following completed
`native-engine-browser-517`, following completed
`native-engine-browser-516`, following completed
`native-engine-browser-515`, following completed
`native-engine-browser-514`, following completed
`native-engine-browser-513`, following completed
`native-engine-browser-512`, following completed
`native-engine-browser-511`, following completed
`native-engine-browser-510`, following completed
`native-engine-browser-509`, following completed
`native-engine-browser-508`, following completed
`native-engine-browser-507`, following completed
`native-engine-browser-506`, following completed
`native-engine-browser-505`, following completed
`native-engine-browser-504`, following completed
`native-engine-browser-503`, following completed
`native-engine-browser-502`, following completed
`native-engine-browser-501`, following completed
`native-engine-browser-500`, following completed
`native-engine-browser-499`, following completed
`native-engine-browser-498`, following completed
`native-engine-browser-497`, following completed
`native-engine-browser-496`, following completed
`native-engine-browser-495`, following completed
`native-engine-browser-494`, following completed
`native-engine-browser-493`, following completed
`native-engine-browser-492`, following completed
`native-engine-browser-491`, following completed
`native-engine-browser-490`, following completed
`native-engine-browser-489`, following completed
`native-engine-browser-488`, following completed
`native-engine-browser-487`, following completed
`native-engine-browser-486`, following completed
`native-engine-browser-485`, following completed
`native-engine-browser-484`, following completed
`native-engine-browser-483`, following completed
`native-engine-browser-482`, following completed
`native-engine-browser-481`, following completed
`native-engine-browser-480`, following completed
`native-engine-browser-479`, following completed
`native-engine-browser-478`, following completed
`native-engine-browser-477`, following completed
`native-engine-browser-476`, following completed
`native-engine-browser-475`, following completed
`native-engine-browser-474`, following completed
`native-engine-browser-473`, following completed
`native-engine-browser-472`, following completed
`native-engine-browser-471`, following completed
`native-engine-browser-470`, following completed
`native-engine-browser-469`, following completed
`native-engine-browser-468`, following completed
`native-engine-browser-467`, following completed
`native-engine-browser-466`, following completed
`native-engine-browser-465`, following completed
`native-engine-browser-464`, following completed
`native-engine-browser-463`, following completed
`native-engine-browser-462`, following completed
`native-engine-browser-461`, following completed
`native-engine-browser-460`, following completed
`native-engine-browser-459`, following completed
`native-engine-browser-458`, following completed
`native-engine-browser-457`, following completed
`native-engine-browser-456`, following completed
`native-engine-browser-455`, following completed
`native-engine-browser-454`, following completed
`native-engine-browser-453`, following completed
`native-engine-browser-452`, following completed
`native-engine-browser-451`, following completed
`native-engine-browser-450`, following completed
`native-engine-browser-449`, following completed
`native-engine-browser-448`, following completed
`native-engine-browser-447`, following completed
`native-engine-browser-446`, following completed
`native-engine-browser-445`, following completed
`native-engine-browser-444`, following completed
`native-engine-browser-443`, following completed
`native-engine-browser-442`, following completed
`native-engine-browser-441`, following completed
`native-engine-browser-440`, following completed
`native-engine-browser-439`, following completed
`native-engine-browser-438`, following completed
`native-engine-browser-437`, following completed
`native-engine-browser-436`, following completed
`native-engine-browser-435`, following completed
`native-engine-browser-434`, following completed
`native-engine-browser-433`, following completed
`native-engine-browser-432`, following completed
`native-engine-browser-431`,
`native-engine-browser-423`,
`native-engine-browser-422` and `native-engine-browser-420`. Native is now the default runtime for
feature-enabled CLI, MCP, and TUI browser entrypoints; Chromium/CDP is an
explicit migration backend and is never selected as a silent fallback. The
versioned
[Glass Core Web Profile](native-engine-browser-profile.md) is the M0 contract;
the authoritative epic is
[issue #40](https://github.com/wanazhar/glass/issues/40). The design contract,
module decomposition, integration enumeration, and tradeoffs are in the
[native-engine architecture](../architecture/native-engine.md) and
[native-engine analysis](analysis/native-engine.md).

Slice 492 adds ordered `@font-face` source resolution. The native parser keeps
bounded `local()` and URL candidates in CSS order; both browser owners resolve
matching deterministic system faces before trying later admitted sources, so a
missing local face no longer prevents URL fallback. Full installed-font
discovery, FontFace/FontFaceSet event timing, variable/color fonts, and
complete text/Web IDL parity remain open issue #40 gates.

Slice 493 adds the page-realm `FontFace`/`FontFaceSet` surface. Host-projected
CSS descriptors now back persistent `document.fonts` faces across document
snapshots, including status/ready/size, family-aware check/load, dynamic-set
add/delete/clear and iteration, plus loading/loadingdone/loadingerror events
for newly observed CSS faces.

Slice 494 admits script-created `FontFace.load()` sources. Bounded `local()`
sources resolve from the deterministic system font book; `data:` and runtime
Blob URLs are decoded locally; and other URL sources use the existing native
Fetch event-loop bridge. Admitted bytes cross a bounded `FontFaceInstall`
command and are parser-validated before entering the document font book. The
URL path currently inherits Fetch's `connect-src` policy rather than the
dedicated `font-src` loader policy, and has no host acknowledgement after
installation; both are explicit next issue #40 gates. Installed-font discovery,
font-display timing, variable/color fonts, cross-realm FontFace projection,
and complete text/Web IDL parity remain open issue #40 gates.

Slice 495 routes dynamic FontFace URL sources through a private `font` Fetch
destination. The content owner validates the destination and invokes the
existing native font loader, restoring document `font-src`, mixed-content,
redirect, CORS, cookie, cache, MIME, and bounded-byte enforcement before
projecting the admitted bytes through the existing Fetch resolver and
`FontFaceInstall` command. Service Worker interception for dynamic FontFace
requests, host acknowledgement of install admission, final URL/response-header
fidelity, installed-font discovery, font-display timing, variable/color fonts,
cross-realm FontFace projection, and complete text/Web IDL parity remain open
issue #40 gates.

The completed FontFace admission-acknowledgement follow-up is
[`native-engine-browser-496`](tasks/native-engine-browser-496.md): page
`FontFace.load()` promises remain pending until the native document owner has
validated and admitted the bounded `FontFaceInstall` command. The shared helper
drains bounded Promise continuations after acknowledgement and is wired through
initial/dynamic page scripts plus local and content-process mutation paths.
Supported page-script continuation fetch, WebSocket, EventSource, scroll, and
navigation effects retain their owner handoff; arbitrary network effects from
standalone content user-event mutations remain outside that response contract.
Service Worker interception, final response metadata, broader FontFace sources,
installed-font discovery, font-display timing, variable/color fonts,
cross-realm projection, and complete text/Web IDL parity remain issue #40 gates.

The completed FontFace admission-rejection follow-up is
[`native-engine-browser-497`](tasks/native-engine-browser-497.md): failed
parser, descriptor, byte, or aggregate-resource admission leaves the live
document font book unchanged and returns a bounded error acknowledgement. The
page face now transitions to `error`, rejects `load()`, and settles its
`FontFaceSet` loading cycle; unrelated DOM command failures remain errors at
their original owner boundary. Broader FontFace sources, Service Worker
interception, response metadata, installed-font discovery, font-display
timing, variable/color fonts, cross-realm projection, and complete FontFace/Web
IDL parity remain issue #40 gates.

The completed FontFace source-list follow-up is
[`native-engine-browser-498`](tasks/native-engine-browser-498.md): bounded
ordered `url()`/`local()` candidates now honor supported `format()` descriptors
and advance through unavailable or rejected candidates until one succeeds.
Quoted/function-contained commas remain intact, and successful bytes still use
the transactional host acknowledgement path. `tech()` descriptors,
BufferSource constructor inputs, richer CSS tokenization, variable/color
negotiation, installed-font discovery, and complete FontFace/Web IDL parity
remain issue #40 gates.

The completed FontFace BufferSource follow-up is
[`native-engine-browser-499`](tasks/native-engine-browser-499.md):
`ArrayBuffer` and `ArrayBufferView` constructor inputs are copied at the exact
selected byte range and admitted through the existing transactional
`FontFaceInstall` path. Empty, oversized, and detached inputs fail explicitly;
typed-array identity, `tech()` descriptors, richer CSS tokenization,
variable/color negotiation, installed-font discovery, and complete
FontFace/Web IDL parity remain issue #40 gates.

The completed FontFace technology-descriptor follow-up is
[`native-engine-browser-500`](tasks/native-engine-browser-500.md): bounded
`tech()` requirements are parsed and fail closed against the native renderer's
currently empty technology set, allowing later ordinary sources to load while
preventing unsupported color/variable technology from being admitted. The
existing ordered fallback and transactional host acknowledgement remain in
force; technology negotiation and complete FontFace/Web IDL parity remain
issue #40 gates.

The completed installed-font discovery follow-up is
[`native-engine-browser-501`](tasks/native-engine-browser-501.md): the native
font book searches bounded user and platform roots for TTF, OTF, TTC, and OTC
files, reads collection metadata with `ttf-parser`, preserves static faces
first, and admits deterministic family/style matches under explicit file,
byte, collection, and face limits. Eager discovery cost, variable/color
tables, WOFF, font-display timing, and complete FontFace/Web IDL parity
remain issue #40 gates.

The completed WOFF admission follow-up is
[`native-engine-browser-502`](tasks/native-engine-browser-502.md): bounded WOFF
1.0 resources are validated, zlib-decompressed when needed, reconstructed as
SFNT bytes, checksum-repaired, and admitted through the existing raster and
shaping owners. The page source filter now fails closed for unsupported variation,
color, EOT, and SVG formats that the native renderer does not yet implement.
Decoded-size limits, variable/color rendering, and complete font-format
and FontFace/Web IDL parity remain issue #40 gates.

The completed CSS unicode-range selection follow-up is
[`native-engine-browser-503`](tasks/native-engine-browser-503.md): bounded
codepoint, range, and wildcard descriptors are normalized and carried through
the page/content font-resource boundary, while tied named-family faces are
selected by both range membership and glyph coverage. The `document.fonts`
projection exposes the canonical range. Mixed-script shaping, font-stretch and
variant descriptors, font-display timing, variable/color fonts, and
complete FontFace/Web IDL parity remain issue #40 gates.

The completed dynamic FontFace unicode-range follow-up is
[`native-engine-browser-504`](tasks/native-engine-browser-504.md): the page
realm now carries `FontFace`'s `unicodeRange` through the bounded install
command, and the native document owner applies the shared parser, normalized
range storage, and 32-range admission limit before font bytes are installed.
Malformed descriptors are rejected transactionally; omitted command fields
retain unrestricted coverage for compatibility. Mixed-script shaping,
font-stretch and variant descriptors, font-display timing, variable/color
fonts, cross-realm FontFace projection, and complete FontFace/Web IDL
parity remain issue #40 gates.

The completed `font-stretch` descriptor follow-up is
[`native-engine-browser-505`](tasks/native-engine-browser-505.md): bounded
named and 50%–200% percentage ranges now cross CSS and script-created
FontFace resource boundaries, content-process wire snapshots, and the
`document.fonts` projection. Invalid or reversed values fail before byte
admission and older payloads default to `normal`. The computed CSS property,
face-range matching, horizontal glyph scaling, and inherited font-variant
controls are now represented in the native rendering path; font-display
timing, variable/color fonts, cross-realm FontFace projection, and
complete FontFace/Web IDL parity remain issue #40 gates.

The completed rendering follow-up is
[`native-engine-browser-506`](tasks/native-engine-browser-506.md): computed
`font-stretch` now inherits and projects as a canonical percentage, matching
considers the closest bounded face range after weight/style, and shaped and
character-path runs scale horizontal advances, offsets, kerning, and glyph
coverage. `local()` source lookup uses the selected descriptor's nominal width;
descriptor ranges remain on a normal-width synthetic baseline until a real
variation-axis owner exists. Variable/color tables, font-display timing,
mixed-script shaping, cross-realm FontFace projection, and complete FontFace/Web
IDL parity remain issue #40 gates.

The completed ligature-shaping follow-up is
[`native-engine-browser-507`](tasks/native-engine-browser-507.md):
`font-variant-ligatures` now inherits through the native cascade, projects as a
canonical CSSOM value, and controls HarfRust's common, discretionary,
historical, and contextual OpenType features during measurement and
rasterization. The broader `font-variant` family, variable/color fonts,
font-display timing, mixed-script shaping, cross-realm FontFace projection, and
complete FontFace/Web IDL parity remain issue #40 gates.

The completed low-level feature-settings follow-up is
[`native-engine-browser-508`](tasks/native-engine-browser-508.md):
`font-feature-settings` now inherits through the native cascade, projects as a
canonical CSSOM list, and supplies bounded quoted OpenType tags to HarfRust
for measurement and rasterization. Duplicate tags normalize last-wins, and
explicit ligature-tag values override the native `font-variant-ligatures`
defaults. The fixed cap and escape-free parser are deliberate boundaries;
variable/color fonts, font-display timing, mixed-script shaping,
cross-realm FontFace projection, and complete FontFace/Web IDL parity remain
issue #40 gates.

The completed kerning follow-up is
[`native-engine-browser-509`](tasks/native-engine-browser-509.md): inherited
`font-kerning` now projects through native CSSOM and maps `auto`, `normal`, and
`none` to the existing HarfRust `kern` feature owner, while an explicit
`"kern"` setting retains precedence. Character fallback keeps its bounded
advances; variable/color fonts, font-display timing, mixed-script
shaping, cross-realm FontFace projection, and complete FontFace/Web IDL parity
remain issue #40 gates.

The completed `font-variant-caps` follow-up is
[`native-engine-browser-510`](tasks/native-engine-browser-510.md): the
inherited capitalization property is carried through native CSSOM and
mapped to the bounded HarfRust `smcp`, `c2sc`, `pcap`, `c2pc`, `unic`, and
`titl` feature tags, with explicit low-level feature settings retaining
precedence. Character fallback remains bounded; variable/color fonts,
font-display timing, mixed-script shaping, cross-realm FontFace projection,
and complete FontFace/Web IDL parity remain issue #40 gates.

The completed `font-variant-position` follow-up is
[`native-engine-browser-511`](tasks/native-engine-browser-511.md): inherited
`normal`, `sub`, and `super` values now cross native CSSOM and map to the
bounded HarfRust `subs` and `sups` feature tags, with explicit low-level
feature settings retaining precedence. Character fallback and typographic
baseline metrics remain bounded; numeric variants, variable/color fonts,
font-display timing, mixed-script shaping, cross-realm FontFace
projection, and complete FontFace/Web IDL parity remain issue #40 gates.

The completed automatic variable-axis follow-up is
[`native-engine-browser-520`](tasks/native-engine-browser-520.md): selected
variable faces now receive automatic `wght=400|700` and CSS-stretch-derived
`wdth` coordinates when those axes are advertised and no explicit coordinate
overrides them. The effective values feed both HarfRust shaping and the
variation-aware outline rasterizer; descriptor and authored coordinates retain
priority, and synthetic scaling remains for compatibility. Optical sizing,
custom axes, hinting, color tables, and complete FontFace/Web IDL parity remain
issue #40 gates.

The completed numeric font-weight follow-up is
[`native-engine-browser-521`](tasks/native-engine-browser-521.md): CSS integer
weights from 1 through 1000 now survive parsing, inheritance, cascade,
`@font-face` and script-created `FontFace` descriptors, local-font selection,
CSSOM, face scoring, and paint selection. Numeric requests feed an advertised
variable `wght` axis, while `normal` and `bold` retain their canonical 400 and
700 aliases and explicit variation coordinates retain priority. Custom axes,
hinting, color tables, and complete FontFace/Web IDL parity remain issue #40
gates.

The completed optical-sizing follow-up is
[`native-engine-browser-522`](tasks/native-engine-browser-522.md): inherited
`font-optical-sizing: auto|none` now survives parsing, cascade, computed style,
and CSSOM projection. `auto` supplies a bounded computed font-size coordinate
to an advertised OpenType `opsz` axis, while authored element coordinates and
`@font-face`/script-created `FontFace` descriptors retain precedence; the
effective value reaches both HarfRust shaping and variation-aware outline
rasterization. Custom axes, hinting, color tables, and complete FontFace/Web IDL
parity remain issue #40 gates.

The completed bounded bitmap-glyph follow-up is
[`native-engine-browser-532`](tasks/native-engine-browser-532.md): the native
text renderer now consumes bounded `ttf-parser` raster strikes from selected
font faces, including TTC/OTC collection indices. PNG, premultiplied BGRA32,
monochrome, and 2/4/8-bit grayscale payloads retain offsets and strike scaling
under finite dimension/byte budgets; color pixels carry embedded RGBA alpha
through the existing compositor while grayscale images reuse text-paint
coverage. Malformed, unsupported, oversized, and over-stretched bitmap data
falls back to the existing outline paths. Bitmap variation axes, hinting,
font-display timing, and complete FontFace/Web IDL parity remain issue #40
gates.

The completed COLR palette-plumbing follow-up is
[`native-engine-browser-537`](tasks/native-engine-browser-537.md): native text
metrics now carry a bounded face-local CPAL palette index into COLR painting,
with out-of-range indices failing closed before malformed or unsupported paint
data can enter the raster path. Palette zero remains the default boundary for
unregistered or malformed CSS palette selections; keyword and named base
palette paths are recorded by slices 538 and 539.

The completed SVG-font follow-up is
[`native-engine-browser-541`](tasks/native-engine-browser-541.md): bounded
OpenType `SVG ` glyph documents now accept UTF-8 or gzip SVGZ, map finite
viewBoxes into font-size pixels, and reuse the native SVG surface rasterizer.
Scripts, external resources, imports, unsupported references, malformed
documents, and over-budget payloads fail closed to existing COLR, bitmap,
variable-outline, or fontdue-outline fallback.

The completed CSS font-display follow-up is
[`native-engine-browser-542`](tasks/native-engine-browser-542.md): bounded
`font-display` descriptors now accept the five normalized CSS keywords and
project through static `document.fonts` faces as `FontFace.display`, with the
descriptor included in static-face identity. Resource admission and status
semantics remain unchanged; actual block/swap/fallback/optional render timing,
installed-font discovery, and complete FontFace/Web IDL parity remain issue #40
gates.

The completed dynamic FontFace Service Worker follow-up is
[`native-engine-browser-543`](tasks/native-engine-browser-543.md): controlled
private `font` fetches now reach the native Service Worker with
`Request.destination === "font"`, preserve response metadata and document
`font-src` policy, and fall back to the existing direct font loader when
unhandled. Font-display timing, installed-font discovery, and complete
FontFace/Web IDL parity remain issue #40 gates.

The completed FontFace display-validation follow-up is
[`native-engine-browser-544`](tasks/native-engine-browser-544.md): dynamic
`FontFace.display` constructor and setter values now normalize to the five
bounded CSS keywords and reject invalid values transactionally with
`SyntaxError`. Static CSS projection, resource admission, Service Worker
routing, and loading status remain unchanged; actual display timing,
installed-font discovery, and complete FontFace/Web IDL parity remain issue #40
gates.

The completed direct FontFace response-metadata follow-up is
[`native-engine-browser-545`](tasks/native-engine-browser-545.md): unhandled
dynamic network font sources now retain the native final URL, status, exposed
headers, content type, redirect bit, and bounded body through the page fetch
payload. Fresh font-cache and HTTP 304 paths preserve the same metadata while
the CSS `@font-face` byte-only loader API remains unchanged. Font-display
timing, installed-font discovery, and complete FontFace/Web IDL parity remain
issue #40 gates.

The completed FontFace descriptor-validation follow-up is
[`native-engine-browser-546`](tasks/native-engine-browser-546.md): dynamic and
static-projected `FontFace` descriptors now validate and normalize the native
`style`, `weight`, `stretch`, `unicodeRange`, and `variationSettings` grammar
at construction/setter boundaries. Invalid or over-budget values fail
transactionally before `load()`, while valid normalized descriptors reach the
native install command. Variant/feature-setting parity, display timing,
installed-font discovery, and complete FontFace/Web IDL parity remain issue #40
gates.

The completed bounded attribute case-sensitivity follow-up is
[`native-engine-browser-647`](tasks/native-engine-browser-647.md): native
attribute selectors now parse explicit `i` ASCII-insensitive and `s`
case-sensitive value flags across exact, token, dash-match, prefix, suffix,
and substring operators. Matching feeds CSS action locators and stylesheet
cascade while malformed or unseparated modifiers remain fail-closed. Full
selector grammar, default enumerated-attribute semantics, namespaces,
pseudo-elements, complete CSS parity, and complete Web IDL parity remain issue
#40 gates.

The current bounded JavaScript structural-correction checkpoint completes
`:first-of-type` and `:only-child` matching and adds negative/positive
tree-order coverage. Structural pseudo-classes now reject non-leading
same-type elements and multi-child elements instead of passing through the
unsupported-selector allowlist; complete JavaScript selector grammar, complete
CSS parity, and complete Web IDL parity remain issue #40 gates.

The current bounded JavaScript hover-selector checkpoint projects durable
native hover state through the DOM snapshot and aligns page-realm `:hover`
matching with native ancestor-chain behavior. Hovered targets, ancestors, and
query-selector results remain coherent after semantic pointer movement;
unsupported selector grammar remains fail-closed. Complete JavaScript selector
grammar, complete CSS parity, and complete Web IDL parity remain issue #40
gates.

The current bounded JavaScript language-direction selector checkpoint aligns
page-realm `:lang()` and `:dir()` with native inherited language and
direction state. HTML `lang`, XML language namespaces, wildcard/prefix
language ranges, nearest `dir` attributes, and default LTR matching use the
projected ancestor chain; unsupported selector grammar remains fail-closed.
Complete JavaScript selector grammar, complete CSS parity, and complete Web
IDL parity remain issue #40 gates.

The current bounded JavaScript relational-selector checkpoint aligns
page-realm `:has()` with native descendant, child, adjacent-sibling, and
general-sibling relations. Relative selector arguments project tree
descendants and sibling order, including compound selectors and selector
lists; unsupported selector grammar remains fail-closed. Complete JavaScript
selector grammar, dynamic control-state parity, complete CSS parity, and
complete Web IDL parity remain issue #40 gates.

The current bounded JavaScript structural-selector checkpoint aligns
page-realm `matches` and query traversal with native tree-order semantics for
`nth-child`, `nth-last-child`, `first/last/only-of-type`, and
`nth-of-type` formulas. Tag parsing no longer consumes pseudo-class suffixes,
and odd/even, integer, and An+B formulas use projected sibling state.
Unsupported selector grammar remains fail-closed; complete JavaScript selector
grammar, dynamic control-state parity, complete CSS parity, and complete Web
IDL parity remain issue #40 gates.

The current bounded JavaScript action-state selector checkpoint aligns
page-realm `matches` and query traversal with native link/any-link, target,
range, and indeterminate state. URL fragments decode into target matching;
numeric and temporal bounds, radio groups, and progress state use projected
snapshots and tree state. Unsupported selector grammar remains fail-closed;
complete JavaScript selector grammar, dynamic control-state parity, complete
CSS parity, and complete Web IDL parity remain issue #40 gates.

The current bounded JavaScript form-state selector checkpoint aligns page-realm
selector matching with native focus, validation, required/optional,
editability, placeholder, and default-control state. `:focus-within` traverses
projected descendants and form validity uses native validity snapshots;
unsupported selector grammar remains fail-closed. Complete JavaScript selector
grammar, dynamic control-state parity, complete CSS parity, and complete Web
IDL parity remain issue #40 gates.

The current bounded user-validation selector checkpoint adds `:user-valid`
and `:user-invalid` from durable native user-interaction state. Semantic
click, typing, clearing, selection, upload, and keyboard edits mark controls
as user interacted; content-process wires and JavaScript snapshots preserve
the state, and native/DOM selectors and stylesheet cascade consume it.
Complete validation event semantics, full selector grammar, complete CSS
parity, and complete Web IDL parity remain issue #40 gates.

The current bounded scope-selector checkpoint adds `:scope` to native
selectors and threads an explicit scoping root through JavaScript `matches`,
`closest`, and query-selector traversal. Document queries use the document
element as scope; element queries use their owner, so `:scope > ...` remains
owner-relative across action locators, stylesheet cascade, and DOM queries.
XML and shadow-root scoping, full selector grammar, complete CSS parity, and
complete Web IDL parity remain issue #40 gates.

The current bounded hover-state selector checkpoint adds `:hover` matching
from semantic hover actions. Native hover state persists on the target and
attached ancestors, crosses local/content-process wire snapshots, clears on
the next hover action, and feeds action selectors and stylesheet cascade.
Pointer movement outside the semantic action surface and full dynamic
pseudo-class timing remain explicit issue #40 gates, alongside complete CSS and
Web IDL parity.

The current bounded indeterminate-state selector checkpoint adds
`:indeterminate` for radio groups with no checked member and `<progress>`
elements with missing, malformed, or negative values. Checkbox indeterminate
property semantics remain fail-closed because the bounded DOM surface does not
expose that mutable state; native matching feeds action selectors and
stylesheet cascade. Full selector grammar, complete CSS parity, and complete
Web IDL parity remain issue #40 gates.

The current bounded URL-target selector checkpoint adds `:target` matching
against the decoded document fragment, including unique `id` targets and
legacy named anchors. Navigation and same-document hash updates refresh the
target state in local and content-process paths; malformed or unresolved
fragments remain fail-closed. Full selector grammar, complete CSS parity, and
complete Web IDL parity remain issue #40 gates.

The current bounded range-state selector checkpoint adds `:in-range` and
`:out-of-range` for non-empty numeric and temporal input values with valid
`min`/`max` constraints. Native range validity feeds action selectors and
stylesheet cascade; unbounded, malformed, empty, disabled, and read-only
controls remain fail-closed. Full selector grammar, complete CSS parity, and
complete Web IDL parity remain issue #40 gates.

The current bounded form-state selector checkpoint adds `:focus`,
`:focus-within`, `:focus-visible`, `:valid`, `:invalid`, `:read-only`,
`:read-write`, `:placeholder-shown`, and `:default` matching. Native focus,
validation, editable-state, placeholder, and default-control state now feeds
action selectors and stylesheet cascade; unsupported user-state, indeterminate,
and pseudo-element forms remain fail-closed. Full selector grammar, complete
CSS parity, and complete Web IDL parity remain issue #40 gates.

The current bounded nth-filter checkpoint adds `of <selector-list>` support to
`:nth-child()` and `:nth-last-child()`. Filtered sibling positions count only
elements matching the selector list, its maximum specificity contributes to
the pseudo-class, and malformed forms remain fail-closed; `of` on typed nth
variants remains unsupported. Full selector grammar, pseudo-elements, complete
CSS parity, and complete Web IDL parity remain issue #40 gates.

The current bounded direction-selector checkpoint adds `:dir(ltr)` and
`:dir(rtl)` matching through inherited HTML `dir` attributes. Missing values
default to left-to-right, explicit `auto` and malformed arguments fail closed,
and the result feeds action selectors and stylesheet cascade. Full selector
grammar, writing-mode/bidi parity, complete CSS parity, and complete Web IDL
parity remain issue #40 gates.

The current bounded language-selector checkpoint adds `:lang(...)` matching
for inherited HTML `lang` and XML `xml:lang` values. ASCII language ranges
support case-insensitive base-tag matching, descendant subtags, and a bounded
terminal wildcard; malformed or unsupported forms remain fail-closed. Full
selector grammar, pseudo-elements, complete CSS parity, and complete Web IDL
parity remain issue #40 gates.

The current bounded namespace-declaration checkpoint adds supported
`@namespace` statements for the HTML, SVG, MathML, XML, XMLNS, and XLink URI
constants. Prefix and default bindings flow through stylesheet type and
attribute selectors, including nested functional selectors; unknown namespace
URIs, prefixes, and malformed forms remain diagnostic and fail-closed. Full
namespace grammar, selector caching, complete CSS parity, and complete Web
IDL parity remain issue #40 gates.

The completed bounded attribute-namespace follow-up is
[`native-engine-browser-649`](tasks/native-engine-browser-649.md): native
attribute selectors now support built-in `xlink`, `xml`, `xmlns`, `svg`,
`html`, and `math` prefixes plus wildcard and no-namespace forms. Matching
uses retained DOM attribute namespace metadata for action locators and
stylesheet cascade; malformed and unknown namespace forms remain fail-closed.
Full `@namespace` declaration composition, complete attribute namespace
grammar, selector caching, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.

The completed bounded namespace type-selector follow-up is
[`native-engine-browser-648`](tasks/native-engine-browser-648.md): native
selectors now support bounded `svg|name`, `html|name`, `math|name`, `*|name`,
`|name`, and namespace-qualified universal forms against the attached DOM.
Matching feeds CSS action locators and stylesheet cascade; unknown prefixes and
malformed namespace forms remain fail-closed. Full namespace declaration and
attribute-namespace grammar, selector caching, complete CSS parity, and
complete Web IDL parity remain issue #40 gates.

The completed bounded typed-structure selector follow-up is
[`native-engine-browser-646`](tasks/native-engine-browser-646.md): native
selectors now support `:first-of-type`, `:last-of-type`, and `:only-of-type`
through attached-document typed sibling positions. Matching feeds CSS action
locators and stylesheet cascade while unsupported dynamic and pseudo-element
forms remain fail-closed. Full selector grammar, case-sensitivity flags,
namespaces, pseudo-elements, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.

The completed bounded attribute-selector follow-up is
[`native-engine-browser-645`](tasks/native-engine-browser-645.md): native
selectors now support presence, exact, whitespace-token (`~=`), language
dash-match (`|=`), prefix (`^=`), suffix (`$=`), and substring (`*=`)
attribute operators. Document-aware matching feeds CSS action locators and
stylesheet cascade, while malformed operators and values remain fail-closed.
Full selector grammar, case-sensitivity flags, namespaces, pseudo-elements,
complete CSS parity, and complete Web IDL parity remain issue #40 gates.

The completed bounded nth-selector follow-up is
[`native-engine-browser-644`](tasks/native-engine-browser-644.md): native
selectors now support bounded `:nth-child()`, `:nth-last-child()`,
`:nth-of-type()`, and `:nth-last-of-type()` formulas, including integer,
`odd`/`even`, and `an+b` forms. Document-aware sibling positions feed CSS
action locators and stylesheet cascade; malformed formulas and unsupported
`of` clauses remain fail-closed. Full selector grammar, namespaces,
pseudo-elements, complete CSS parity, and complete Web IDL parity remain issue
#40 gates.

The completed bounded relative-selector follow-up is
[`native-engine-browser-643`](tasks/native-engine-browser-643.md): native
`:has(...)` now accepts bounded relative descendant, child (`>`), adjacent
sibling (`+`), and subsequent-sibling (`~`) arguments. Document-aware matching
feeds both CSS action locators and stylesheet cascade, with bounded depth and
fail-closed malformed arguments. Full selector grammar, nth arithmetic,
namespaces, pseudo-elements, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.

The completed bounded functional-selector follow-up is
[`native-engine-browser-642`](tasks/native-engine-browser-642.md): native
selectors now support bounded `:not(...)`, `:is(...)`, and `:where(...)`
argument lists, nested functional matching, and CSS-specificity rules for
negation, alternation, and zero-specificity `:where`. Stylesheet selector-list
parsing preserves commas inside functional arguments, while unsupported
functional forms and malformed syntax remain fail-closed. Full selector
grammar, `:has()` relative selectors, nth arithmetic, namespaces,
pseudo-elements, complete CSS parity, and complete Web IDL parity remain issue
#40 gates.

The completed bounded selector-combinator follow-up is
[`native-engine-browser-641`](tasks/native-engine-browser-641.md): native
selector chains now retain descendant, child (`>`), adjacent-sibling (`+`),
and subsequent-sibling (`~`) relationships. Matching advances through the
attached document tree for mixed combinator chains, with bounded parser limits
and fail-closed malformed-combinator handling preserved. Full selector
grammar, namespaces, functional selectors, pseudo-elements, complete CSS
parity, and complete Web IDL parity remain issue #40 gates.

The completed bounded pseudo-class selector follow-up is
[`native-engine-browser-640`](tasks/native-engine-browser-640.md): native
compound selectors now parse bounded structural pseudo-classes (`:root`,
`:first-child`, `:last-child`, `:only-child`, `:empty`) plus bounded state and
link forms (`:checked`, `:disabled`, `:enabled`, `:required`, `:optional`,
`:link`, and `:any-link`). Document-aware matching feeds both CSS action
locators and stylesheet cascade, while unsupported functional, dynamic, and
pseudo-element forms remain fail-closed. Full selector grammar, child/sibling
combinators, pseudo-elements, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.

The completed inline supports-rule follow-up is
[`native-engine-browser-639`](tasks/native-engine-browser-639.md): native
stylesheets now retain bounded `@supports` conditions, including
media-plus-supports composition, and filter both declarations and
custom-property candidates through the existing supports evaluator. Repeated
nested same-kind conditional rules remain explicitly diagnosed. Full
Conditional Rules grammar, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.

The completed inline media-rule follow-up is
[`native-engine-browser-638`](tasks/native-engine-browser-638.md): native
stylesheets now retain bounded `@media` conditions on parsed rules and apply
the existing viewport media evaluator during both declaration and
custom-property cascade. Inactive media rules remain fail-closed, while
`@layer` propagation is preserved and nested media remains explicitly
diagnosed. Full Media Queries grammar, nested conditional composition, full
CSS parity, and complete Web IDL parity remain issue #40 gates.

The completed physical border color shorthand variable follow-up is
[`native-engine-browser-637`](tasks/native-engine-browser-637.md): native
`border-color` now retains side-indexed shorthand candidates through bounded
`var(--name)` aliases and expands concrete one- to four-color fallbacks using
the existing physical edge grammar. Invalid and cyclic handling plus
declaration-order precedence remain fail-closed. Full registered properties,
complete color grammar, and complete CSS and Web IDL parity remain issue #40
gates.

The completed logical border color variable follow-up is
[`native-engine-browser-636`](tasks/native-engine-browser-636.md): native
`border-block-color` and `border-inline-color` now preserve logical side
indices through bounded `var(--name)` alias chains and concrete two-color
fallbacks before direction-aware physical projection. Invalid and cyclic
handling plus declaration-order precedence remain fail-closed. Full registered
properties, complete color grammar, and complete CSS and Web IDL parity remain
issue #40 gates.

The completed grid-template substitution follow-up is
[`native-engine-browser-635`](tasks/native-engine-browser-635.md): native
`grid-template-columns` and `grid-template-rows` now accept bounded standalone
`var(--name)` values and concrete track-list fallbacks. Resolution preserves
inherited aliases, invalid and cyclic handling, `none`, repeat expansion, and
declaration-order precedence. Full grid grammar, registered properties, and
complete CSS and Web IDL parity remain issue #40 gates.

The completed background shorthand substitution follow-up is
[`native-engine-browser-634`](tasks/native-engine-browser-634.md): native
`background` now accepts bounded standalone `var(--name)` values and concrete
full-shorthand fallbacks. Resolution expands through color, image, repeat,
position, and size while preserving inherited aliases, invalid and cyclic
handling, and declaration-order precedence. Full background grammar,
registered properties, and complete CSS and Web IDL parity remain issue #40
gates.

The completed background-size substitution follow-up is
[`native-engine-browser-633`](tasks/native-engine-browser-633.md): native
`background-size` now accepts bounded standalone `var(--name)` values and
concrete size, `cover`, or `contain` fallbacks. Resolution preserves inherited
aliases, invalid and cyclic fallback handling, bounded pixel and percentage
components, and declaration-order precedence. Full size grammar, registered
properties, and complete CSS and Web IDL parity remain issue #40 gates.

The completed background-position substitution follow-up is
[`native-engine-browser-632`](tasks/native-engine-browser-632.md): native
`background-position` now accepts bounded standalone `var(--name)` values and
concrete axis fallbacks. Resolution preserves inherited aliases, invalid and
cyclic fallback handling, bounded pixel and percentage components, and
declaration-order precedence. Full position grammar, registered properties,
and complete CSS and Web IDL parity remain issue #40 gates.

The completed background-repeat substitution follow-up is
[`native-engine-browser-631`](tasks/native-engine-browser-631.md): native
`background-repeat` now accepts bounded standalone `var(--name)` values and
normalized repeat-mode fallbacks. Resolution preserves inherited aliases,
invalid and cyclic fallback handling, and declaration-order precedence. Full
repeat-list grammar, registered properties, and complete CSS and Web IDL
parity remain issue #40 gates.


The completed background-image substitution follow-up is
[`native-engine-browser-630`](tasks/native-engine-browser-630.md): native
`background-image` now accepts bounded standalone `var(--name)` values and
concrete URL or `none` fallbacks. Resolution preserves inherited aliases,
invalid and cyclic fallback handling, explicit `none`, and declaration-order
precedence. Full image-list grammar, registered properties, and complete CSS
and Web IDL parity remain issue #40 gates.

The completed text-decoration shorthand substitution follow-up is
[`native-engine-browser-629`](tasks/native-engine-browser-629.md): native
`text-decoration` and `text-decoration-line` now accept bounded standalone
`var(--name)` values and concrete line-set fallbacks. Resolution preserves
inherited aliases, invalid and cyclic fallback handling, reset,
declaration-order precedence, and `revert-layer` rollback. Full custom-
property grammar, registered properties, and complete CSS and Web IDL parity
remain issue #40 gates.

The completed text-decoration presentation substitution follow-up is
[`native-engine-browser-628`](tasks/native-engine-browser-628.md): native
`text-decoration-style`, `text-decoration-skip-ink`,
`text-decoration-skip-spaces`, `text-decoration-thickness`, and
`text-underline-offset` now accept bounded standalone `var(--name)` values
and concrete fallbacks. Resolution preserves inherited aliases, invalid and
cyclic fallback handling, reset and `revert-layer` behavior, and declaration
order. Full custom-property grammar, registered properties, and complete CSS
and Web IDL parity remain issue #40 gates.

The completed `place-content` substitution follow-up is
[`native-engine-browser-627`](tasks/native-engine-browser-627.md): native
`place-content` now accepts bounded standalone `var(--name)` values and
concrete one- or two-value fallbacks, projecting through the existing
`align-content` and `justify-content` components. Resolution preserves
inherited aliases, invalid and cyclic fallback handling, reset and
`revert-layer` behavior, and same-block longhand precedence. Full custom-
property grammar, registered properties, and complete CSS and Web IDL parity
remain issue #40 gates.

The completed flex alignment substitution follow-up is
[`native-engine-browser-626`](tasks/native-engine-browser-626.md): native
`justify-content`, `align-items`, `align-self`, and `align-content` now accept
bounded standalone `var(--name)` values and concrete keyword fallbacks.
Resolution projects inherited aliases through non-inherited alignment state,
handles invalid and cyclic values, preserves declaration-order precedence,
and keeps existing reset, inherit, and `revert-layer` behavior. Full
custom-property grammar, registered properties, and complete CSS and Web IDL
parity remain issue #40 gates.

The completed flex order substitution follow-up is
[`native-engine-browser-625`](tasks/native-engine-browser-625.md): native
`order` now accepts bounded standalone `var(--name)` values and signed integer
fallbacks. Resolution projects inherited aliases through non-inherited flex
item order, handles invalid and cyclic values, preserves declaration-order
precedence, and keeps existing reset and `revert-layer` behavior. Full
custom-property grammar, registered properties, and complete CSS and Web IDL
parity remain issue #40 gates.

The completed flex shorthand substitution follow-up is
[`native-engine-browser-624`](tasks/native-engine-browser-624.md): native
`flex` now accepts bounded standalone `var(--name)` values and concrete
three-component shorthand fallbacks. Resolution projects inherited aliases
through grow, shrink, and basis components, handles invalid and cyclic values,
preserves longhand override precedence, and keeps existing reset and
`revert-layer` behavior. Full custom-property grammar, registered properties,
and complete CSS and Web IDL parity remain issue #40 gates.

The completed flex sizing substitution follow-up is
[`native-engine-browser-623`](tasks/native-engine-browser-623.md): native
`flex-grow`, `flex-shrink`, and `flex-basis` now accept bounded standalone
`var(--name)` values and concrete numeric, `auto`, or pixel fallbacks.
Resolution projects inherited custom-property aliases, handles invalid and
cyclic fallback values, preserves non-inherited flex sizing, reset behavior,
and declaration-order precedence. Full custom-property grammar, registered
properties, flex shorthand substitution, and complete CSS and Web IDL parity
remain issue #40 gates.

The completed flex-flow substitution follow-up is
[`native-engine-browser-622`](tasks/native-engine-browser-622.md): native
`flex-direction`, `flex-wrap`, and `flex-flow` now accept bounded standalone
`var(--name)` values, with concrete direction, wrap, and flow fallbacks.
Resolution projects inherited custom-property aliases through both flex-flow
components, handles invalid and cyclic fallback values, preserves longhand
override precedence, and keeps existing reset, inherit, and `revert-layer`
behavior. Full custom-property grammar, registered properties, and complete
CSS and Web IDL parity remain issue #40 gates.

The completed visibility substitution follow-up is
[`native-engine-browser-621`](tasks/native-engine-browser-621.md): native
`visibility` now accepts bounded standalone `var(--name)` values and hidden
or visible fallbacks. Resolution covers inherited custom-property aliases,
invalid and cyclic fallback handling, local cascade, and existing
`revert-layer` behavior before the hidden-subtree owner. The existing
hidden/visible grammar remains unchanged; CSS-wide values beyond
`revert-layer`, `collapse`, nested-variable, registered-property, full CSS
variable grammar, and complete CSS and Web IDL parity remain issue #40 gates.

The completed gap substitution follow-up is
[`native-engine-browser-620`](tasks/native-engine-browser-620.md): native
`gap`, `row-gap`, and `column-gap` now accept bounded standalone
`var(--name)` values, with one- or two-value shorthand fallbacks and
single-component fallbacks. Resolution covers inherited custom-property
aliases, invalid and cyclic fallback handling, CSS-wide reset mappings,
independent row/column projection, local cascade, and `revert-layer` without
changing the existing bounded non-negative pixel grammar. Percentage,
relative-unit, calculation, nested-variable, registered-property, full CSS
variable grammar, and complete CSS and Web IDL parity remain issue #40 gates.

The completed complete-border substitution follow-up is
[`native-engine-browser-619`](tasks/native-engine-browser-619.md): native
`border` and the four physical side-border shorthands now accept bounded
standalone `var(--name)` values and concrete-color complete-border fallbacks.
Resolution projects inherited aliases through the width, style, and color
component streams, preserving invalid and cyclic fallback handling, CSS-wide
reset mappings, local cascade, and existing `revert-layer` behavior. The
existing complete-border grammar remains unchanged; current-color, non-
complete fallback forms, nested variable grammar, registered properties, full
CSS variable grammar, and complete CSS and Web IDL parity remain issue #40
gates.

The completed border-radius substitution follow-up is
[`native-engine-browser-618`](tasks/native-engine-browser-618.md): native
`border-radius` and the four physical corner longhands now accept bounded
standalone `var(--name)` and pixel or bounded shorthand fallbacks. Resolution
covers inherited aliases, CSS-wide reset mappings, invalid and cyclic values,
corner-specific fallback resolution, local cascade, `revert-layer`, and
logical-corner projection before computed border geometry. Existing bounded
pixel-only radius grammar remains unchanged; percentage, slash, registered
property, full variable grammar, and complete CSS and Web IDL parity remain
issue #40 gates.

The completed logical border-style substitution follow-up is
[`native-engine-browser-617`](tasks/native-engine-browser-617.md): native
`border-block-style`, `border-block-start-style`, `border-block-end-style`,
`border-inline-style`, `border-inline-start-style`, and
`border-inline-end-style` now accept bounded standalone `var(--name)` and
style-keyword or CSS-wide fallbacks, including logical one-to-two-value pair
expansion. Resolution covers inherited aliases, invalid and cyclic values,
CSS-wide reset mappings, local cascade, direction-aware projection, and
`revert-layer` before border composition. Physical border-style behavior
remains unchanged. Nested variable grammar, registered properties, full CSS
variable grammar, and complete CSS and Web IDL parity remain issue #40 gates.

The completed logical border-width substitution follow-up is
[`native-engine-browser-616`](tasks/native-engine-browser-616.md): native
`border-block-width`, `border-block-start-width`,
`border-block-end-width`, `border-inline-width`,
`border-inline-start-width`, and `border-inline-end-width` now accept bounded
standalone `var(--name)` and pixel or CSS-wide fallbacks, including logical
one-to-two-value pair expansion. Resolution covers inherited aliases, invalid
and cyclic values, CSS-wide reset mappings, local cascade, direction-aware
projection, and `revert-layer` before border composition. Physical border-width
behavior remains unchanged. Nested variable grammar, registered properties,
full CSS variable grammar, and complete CSS and Web IDL parity remain issue #40
gates.

The completed physical border-style substitution follow-up is
[`native-engine-browser-615`](tasks/native-engine-browser-615.md): native
`border-style` and `border-top-style`, `border-right-style`,
`border-bottom-style`, and `border-left-style` now accept bounded standalone
`var(--name)` and style-keyword or CSS-wide fallbacks, including
one-to-four-value shorthand expansion. Resolution covers inherited aliases,
CSS-wide reset mappings, invalid and cyclic values before border composition,
while logical border-style and existing local cascade, `revert-layer`, and
geometry behavior remain unchanged. Nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS and Web IDL parity
remain issue #40 gates.

The completed physical border-width substitution follow-up is
[`native-engine-browser-614`](tasks/native-engine-browser-614.md): native
`border-width` and `border-top-width`, `border-right-width`,
`border-bottom-width`, and `border-left-width` now accept bounded standalone
`var(--name)` and pixel or CSS-wide fallbacks, including one-to-four-value
shorthand expansion. Resolution covers inherited aliases, CSS-wide reset
mappings, invalid and cyclic values before border composition, while logical
border-width and existing local cascade, `revert-layer`, and geometry behavior
remain unchanged. Nested variable grammar, registered properties, full CSS
variable grammar, and complete CSS and Web IDL parity remain issue #40 gates.

The completed margin substitution follow-up is
[`native-engine-browser-613`](tasks/native-engine-browser-613.md): native
physical `margin` shorthand and `margin-top`, `margin-right`, `margin-bottom`,
and `margin-left` now accept bounded standalone `var(--name)` and concrete
pixel or `auto` fallbacks, including one-to-four-value shorthand expansion.
Resolution covers inherited aliases, CSS-wide reset mappings, invalid and
cyclic values, while preserving logical-margin projection, local cascade
precedence, auto-edge provenance, and `revert-layer` rollback. Nested variable
grammar, registered properties, full CSS variable grammar, and complete CSS
and Web IDL parity remain issue #40 gates.

The completed padding substitution follow-up is
[`native-engine-browser-612`](tasks/native-engine-browser-612.md): native
physical `padding` shorthand and longhands now accept bounded standalone
`var(--name)` and concrete pixel fallbacks, including four-value shorthand
expansion, resolving inherited aliases, CSS-wide reset mappings, invalid
values, and cyclic values before computed-style projection. Existing logical
padding, content-box geometry, local cascade precedence, and `revert-layer`
behavior remain unchanged. Nested variable grammar, registered properties,
full CSS variable grammar, and complete CSS/Web IDL parity remain issue #40
gates.

The completed box-sizing substitution follow-up is
[`native-engine-browser-611`](tasks/native-engine-browser-611.md): native
`box-sizing` now accepts bounded standalone `var(--name)` and
`var(--name, content-box|border-box)` fallbacks, resolving inherited aliases,
CSS-wide reset mappings, invalid values, and cyclic values before computed-style
projection. Existing content-box fallback, local cascade precedence, and
`revert-layer` behavior remain unchanged. Nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed dimension substitution follow-up is
[`native-engine-browser-610`](tasks/native-engine-browser-610.md): native
`width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`
now accept bounded standalone `var(--name)` and pixel fallbacks, resolving
inherited aliases, CSS-wide reset mappings, invalid values, and cyclic values
before computed-style projection. Nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed position-offset substitution follow-up is
[`native-engine-browser-609`](tasks/native-engine-browser-609.md): native
`top`, `right`, `bottom`, and `left` now accept bounded standalone
`var(--name)` and pixel or `auto` fallbacks, resolving inherited aliases,
CSS-wide reset mappings, invalid values, and cyclic values before computed
style projection. Nested variable grammar, registered properties, full CSS
variable grammar, and complete CSS/Web IDL parity remain issue #40 gates.

The completed z-index substitution follow-up is
[`native-engine-browser-608`](tasks/native-engine-browser-608.md): native
`z-index` now accepts bounded standalone `var(--name)` and integer or
`auto` fallbacks, resolving inherited aliases, CSS-wide reset mappings,
invalid values, and cyclic values before computed-style projection. Nested
variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity remain issue #40 gates.

The completed position substitution follow-up is
[`native-engine-browser-607`](tasks/native-engine-browser-607.md): native
`position` now accepts bounded standalone `var(--name)` and position-keyword
fallbacks, resolving inherited aliases, CSS-wide reset mappings, invalid
values, and cyclic values before computed-style projection. Nested variable
grammar, registered properties, full CSS variable grammar, and complete CSS/
Web IDL parity remain issue #40 gates.

The completed display substitution follow-up is
[`native-engine-browser-606`](tasks/native-engine-browser-606.md): native
`display` now accepts bounded standalone `var(--name)` and display-keyword
fallbacks, resolving inherited aliases, invalid values, and cyclic values
before computed-style projection. Nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed opacity substitution follow-up is
[`native-engine-browser-605`](tasks/native-engine-browser-605.md): native
`opacity` now accepts bounded standalone `var(--name)` and percentage or
numeric fallbacks, resolving inherited aliases, invalid values, and cyclic
values before computed-style projection. Nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed overflow substitution follow-up is
[`native-engine-browser-604`](tasks/native-engine-browser-604.md): native
`overflow`, `overflow-x`, and `overflow-y` now accept bounded standalone
`var(--name)` and overflow-keyword fallbacks, resolving inherited aliases,
CSS-wide mapped values, invalid values, and cyclic values before clipping and
computed-style projection. Nested variable grammar, registered properties,
full CSS variable grammar, and complete CSS/Web IDL parity remain issue #40
gates.

The completed text-indent substitution follow-up is
[`native-engine-browser-603`](tasks/native-engine-browser-603.md): native
`text-indent` now accepts bounded standalone `var(--name)` and positive-pixel
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before computed-style projection. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed text-overflow substitution follow-up is
[`native-engine-browser-602`](tasks/native-engine-browser-602.md): native
`text-overflow` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before computed-style projection. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited pointer-events substitution follow-up is
[`native-engine-browser-601`](tasks/native-engine-browser-601.md): native
`pointer-events` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before hit-testing targetability. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited letter-spacing substitution follow-up is
[`native-engine-browser-600`](tasks/native-engine-browser-600.md): native
`letter-spacing` now accepts bounded standalone `var(--name)` and positive-pixel
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal letter-spacing computation. Nested variable
grammar, registered properties, full CSS variable grammar, and complete CSS/Web
IDL parity remain issue #40 gates.

The completed inherited word-spacing substitution follow-up is
[`native-engine-browser-599`](tasks/native-engine-browser-599.md): native
`word-spacing` now accepts bounded standalone `var(--name)` and positive-pixel
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal word-spacing computation. Nested variable
grammar, registered properties, full CSS variable grammar, and complete CSS/Web
IDL parity remain issue #40 gates.

The completed inherited vertical-align substitution follow-up is
[`native-engine-browser-598`](tasks/native-engine-browser-598.md): native
`vertical-align` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal vertical alignment. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited word-break substitution follow-up is
[`native-engine-browser-597`](tasks/native-engine-browser-597.md): native
`word-break` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal word-break computation. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited line-height substitution follow-up is
[`native-engine-browser-596`](tasks/native-engine-browser-596.md): native
`line-height` now accepts bounded standalone `var(--name)` and positive-pixel
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal line-height computation. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited white-space substitution follow-up is
[`native-engine-browser-595`](tasks/native-engine-browser-595.md): native
`white-space` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal whitespace handling. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited direction substitution follow-up is
[`native-engine-browser-594`](tasks/native-engine-browser-594.md): native
`direction` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before direction-dependent layout. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited text-justify substitution follow-up is
[`native-engine-browser-593`](tasks/native-engine-browser-593.md): native
`text-justify` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal justification. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited text-align-last substitution follow-up is
[`native-engine-browser-592`](tasks/native-engine-browser-592.md): native
`text-align-last` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal last-line alignment. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited text-align substitution follow-up is
[`native-engine-browser-591`](tasks/native-engine-browser-591.md): native
`text-align` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal alignment. Nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed inherited text-transform substitution follow-up is
[`native-engine-browser-590`](tasks/native-engine-browser-590.md): native
`text-transform` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal text transformation. Nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed `font-variant` shorthand substitution follow-up is
[`native-engine-browser-589`](tasks/native-engine-browser-589.md): native
shorthand declarations now resolve bounded standalone `var(--name)` and
full-shorthand keyword fallbacks into their six inherited OpenType component
values, including aliases, invalid values, cycles, and CSS-wide mapped values.
Nested variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-variant-numeric follow-up is
[`native-engine-browser-588`](tasks/native-engine-browser-588.md): native
`font-variant-numeric` now accepts bounded standalone `var(--name)` and
compound-keyword fallbacks, resolving inherited aliases, CSS-wide mapped
values, invalid values, and cyclic values before normal numeric feature
computation. Complete shorthand substitution, nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-variant-east-asian follow-up is
[`native-engine-browser-587`](tasks/native-engine-browser-587.md): native
`font-variant-east-asian` now accepts bounded standalone `var(--name)` and
compound-keyword fallbacks, resolving inherited aliases, CSS-wide mapped
values, invalid values, and cyclic values before normal East Asian feature
computation. Complete shorthand substitution, nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-variant-alternates follow-up is
[`native-engine-browser-586`](tasks/native-engine-browser-586.md): native
`font-variant-alternates` now accepts bounded standalone `var(--name)` and
keyword fallbacks, resolving inherited aliases, CSS-wide mapped values,
invalid values, and cyclic values before normal alternate computation.
Complete shorthand substitution, nested variable grammar, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed inherited font-variant-position follow-up is
[`native-engine-browser-585`](tasks/native-engine-browser-585.md): native
`font-variant-position` now accepts bounded standalone `var(--name)` and
keyword fallbacks, resolving inherited aliases, CSS-wide mapped values,
invalid values, and cyclic values before normal subscript/superscript
computation. Complete shorthand substitution, nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-variant-caps follow-up is
[`native-engine-browser-584`](tasks/native-engine-browser-584.md): native
`font-variant-caps` now accepts bounded standalone `var(--name)` and keyword
fallbacks, resolving inherited aliases, CSS-wide mapped values, invalid values,
and cyclic values before normal capitalization computation. Complete shorthand
substitution, nested variable grammar, registered properties, full CSS variable
grammar, and complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-variant-ligatures follow-up is
[`native-engine-browser-583`](tasks/native-engine-browser-583.md): native
`font-variant-ligatures` now accepts bounded standalone `var(--name)` and
keyword-list `var(--name, value)` fallbacks, resolving inherited aliases,
CSS-wide mapped values, invalid values, and cyclic values before normal
ligature computation. Complete shorthand substitution, nested variable
grammar, registered properties, full CSS variable grammar, and complete CSS
Web IDL parity remain issue #40 gates.

The completed inherited font-language-override follow-up is
[`native-engine-browser-582`](tasks/native-engine-browser-582.md): native
`font-language-override` now accepts bounded standalone `var(--name)` and
quoted four-byte `var(--name, tag)` fallbacks, resolving inherited aliases,
CSS-wide mapped values, invalid values, and cyclic values before normal
language-override computation. Complete shorthand substitution, nested
variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-variation-settings follow-up is
[`native-engine-browser-581`](tasks/native-engine-browser-581.md): native
`font-variation-settings` now accepts bounded standalone `var(--name)` and
comma-list `var(--name, "tag" value, fallback)` fallbacks, resolving inherited
aliases, CSS-wide mapped values, invalid values, and cyclic values before
normal variation-settings computation. Complete shorthand substitution,
nested variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-feature-settings follow-up is
[`native-engine-browser-580`](tasks/native-engine-browser-580.md): native
`font-feature-settings` now accepts bounded standalone `var(--name)` and
comma-list `var(--name, "tag" value, fallback)` fallbacks, resolving inherited
aliases, CSS-wide mapped values, invalid values, and cyclic values before
normal feature-settings computation. Complete shorthand substitution, nested
variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-palette follow-up is
[`native-engine-browser-579`](tasks/native-engine-browser-579.md): native
`font-palette` now accepts bounded standalone `var(--name)` and concrete
`var(--name, value)` fallbacks, resolving inherited aliases, CSS-wide mapped
values, invalid values, and cyclic values before normal font-palette
computation. Complete shorthand substitution, nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-optical-sizing follow-up is
[`native-engine-browser-578`](tasks/native-engine-browser-578.md): native
`font-optical-sizing` now accepts bounded standalone `var(--name)` and
concrete `var(--name, value)` fallbacks, resolving inherited aliases, CSS-wide
mapped values, invalid values, and cyclic values before normal font-optical-
sizing computation. Complete shorthand substitution, nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-kerning follow-up is
[`native-engine-browser-577`](tasks/native-engine-browser-577.md): native
`font-kerning` now accepts bounded standalone `var(--name)` and concrete
`var(--name, value)` fallbacks, resolving inherited aliases, CSS-wide mapped
values, invalid values, and cyclic values before normal font-kerning
computation. Complete shorthand substitution, nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-family follow-up is
[`native-engine-browser-576`](tasks/native-engine-browser-576.md): native
`font-family` now accepts bounded standalone `var(--name)` and comma-list
`var(--name, family, fallback)` fallbacks, resolving inherited aliases,
CSS-wide mapped values, invalid values, and cyclic values before normal
font-family computation. Complete shorthand substitution, nested variable
grammar, registered properties, installed-font discovery, full CSS variable
grammar, and complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-stretch follow-up is
[`native-engine-browser-575`](tasks/native-engine-browser-575.md): native
`font-stretch` now accepts bounded standalone `var(--name)` and concrete
`var(--name, stretch)` fallbacks, resolving inherited aliases, CSS-wide
mapped values, invalid values, and cyclic values before normal font-stretch
computation. Two-value descriptor ranges, complete shorthand substitution,
non-concrete fallback grammar, registered properties, full CSS variable
grammar, and complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited font-style follow-up is
[`native-engine-browser-574`](tasks/native-engine-browser-574.md): native
`font-style` now accepts bounded standalone `var(--name)` and concrete
`var(--name, style)` fallbacks, resolving inherited aliases, CSS-wide mapped,
invalid, and cyclic values before normal font-style computation. Oblique
angles, complete shorthand substitution, non-concrete fallback grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited font-weight follow-up is
[`native-engine-browser-573`](tasks/native-engine-browser-573.md): native
`font-weight` now accepts bounded standalone `var(--name)` and concrete
`var(--name, weight)` fallbacks, resolving inherited aliases, relative
keywords, CSS-wide mapped values, invalid values, and cyclic values before
normal font-weight computation. Complete shorthand substitution, non-concrete
fallback grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited decoration-color follow-up is
[`native-engine-browser-572`](tasks/native-engine-browser-572.md): native
`text-decoration-color` now accepts bounded standalone `var(--name)` and
concrete `var(--name, color)` fallbacks, resolving inherited, nested, invalid,
and cyclic custom-property values before normal decoration-color resolution.
Complete shorthand substitution, composite color functions, CSS-wide fallback
values, registered properties, broader color properties, full CSS variable
grammar, and complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited border-variable follow-up is
[`native-engine-browser-571`](tasks/native-engine-browser-571.md): native
`border-color` now accepts bounded standalone `var(--name)` and concrete
`var(--name, color)` fallbacks, resolving inherited, nested, invalid, and
cyclic custom-property values before normal border-color resolution. Complete
border shorthand substitution, composite color functions, CSS-wide fallback
values, registered properties, broader color properties, full CSS variable
grammar, and complete CSS/Web IDL parity remain issue #40 gates.

The completed inherited background-variable follow-up is
[`native-engine-browser-570`](tasks/native-engine-browser-570.md): native
`background-color` now accepts bounded standalone `var(--name)` and concrete
`var(--name, color)` fallbacks, resolving inherited, nested, invalid, and
cyclic custom-property values before normal background-color resolution.
Composite color functions, CSS-wide fallback values, registered properties,
broader color properties, full CSS variable grammar, and complete CSS/Web IDL
parity remain issue #40 gates.

The completed inherited color-variable follow-up is
[`native-engine-browser-569`](tasks/native-engine-browser-569.md): native
`color` now accepts bounded standalone `var(--name)` and concrete
`var(--name, color)` fallbacks, resolving inherited, nested, invalid, and
cyclic custom-property values before normal color inheritance. Composite color
functions, CSS-wide fallback values, registered properties, broader color
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed CSSOM recascade follow-up is
[`native-engine-browser-568`](tasks/native-engine-browser-568.md): validated
script-driven attribute writes and removals now invalidate cached native
computed styles, so `CSSStyleDeclaration.setProperty()` and
`removeProperty()` updates recascade inherited custom properties before the
next layout or snapshot. Broader CSSOM mutation semantics, registered
properties, full CSS variable grammar, and complete CSS/Web IDL parity remain
issue #40 gates.

The completed composite CSS variable-calculation follow-up is
[`native-engine-browser-567`](tasks/native-engine-browser-567.md): inherited
native `font-size` now expands bounded custom-property references and
fallbacks inside a complete `calc()` expression before normal checked
fixed-point resolution. Expression bytes, output, nesting, and standalone
custom-property values remain bounded; full CSS token grammar, registered
properties, CSSOM mutation, and complete CSS variable parity remain issue #40
gates.

The completed bounded CSS variable-fallback follow-up is
[`native-engine-browser-566`](tasks/native-engine-browser-566.md): inherited
native `font-size` now accepts one checked `var(--name, fallback)` form,
resolving absolute, relative, viewport, `calc()`, and CSS-wide fallback values
when the referenced custom property is missing or invalid. Composite
substitution inside larger `calc()` expressions, registered properties, CSSOM
mutation, and complete CSS variable parity remain issue #40 gates.

The completed dynamic native viewport follow-up is
[`native-engine-browser-565`](tasks/native-engine-browser-565.md): live native
viewport updates now synchronize the content process, invalidate
viewport-dependent computed styles, advance the document revision, reset root
scroll safely, and expose updated `innerWidth`/`innerHeight` values on the next
page evaluation. Resize events, complete media/image re-selection, and full
viewport-unit/API parity remain issue #40 gates.

The completed chained CSS `calc()` `font-size` follow-up is
[`native-engine-browser-564`](tasks/native-engine-browser-564.md): bounded
left-to-right product chains now combine one deferred font-size dimension
expression with unitless multiplication or division factors, including nested
`calc()` groups, while dimension/dimension products and over-budget chains
fail closed. Dynamic viewport recomputation, full CSS variable grammar, and
complete CSS font-size parity remain issue #40 gates.

The completed bounded CSS custom-property `font-size` follow-up is
[`native-engine-browser-563`](tasks/native-engine-browser-563.md): direct
`var(--name)` references now resolve bounded custom-property values inherited
through matched ancestors and inline overrides with checked recursion and
cascade precedence. Variable fallback syntax, composite `var()` inside
`calc()`, CSSOM mutation, registered properties, and complete CSS variable
parity remain issue #40 gates.

The completed extra-precision CSS `font-size` follow-up is
[`native-engine-browser-562`](tasks/native-engine-browser-562.md): bounded
font-size and `calc()` numbers now accept up to six fractional decimal digits,
round half-up to the existing thousandth coefficients, and preserve the native
integer-pixel result bound and cascade fallback. Full CSS variable grammar,
dynamic viewport recomputation, and complete CSS font-size parity remain issue
#40 gates.

The completed multiplicative CSS `calc()` `font-size` follow-up is
[`native-engine-browser-561`](tasks/native-engine-browser-561.md): bounded
unitless multiplication/division and nested `calc()` groups now scale the
deferred absolute, parent-relative, root-relative, and viewport coefficients,
while dimension/dimension products and invalid factors fail closed. Full CSS
variable grammar, dynamic viewport recomputation, and complete CSS font-size
parity remain issue #40 gates.

The completed additive CSS `calc()` `font-size` follow-up is
[`native-engine-browser-560`](tasks/native-engine-browser-560.md): inherited
element `font-size` now accepts bounded additive and subtractive `calc()`
terms across absolute, parent-relative, root-relative, and viewport units,
with deferred checked resolution and native cascade fallback. Full CSS variable
grammar, dynamic viewport recomputation, and complete CSS font-size parity
remain issue #40 gates.

The completed CSS viewport-relative `font-size` follow-up is
[`native-engine-browser-559`](tasks/native-engine-browser-559.md): inherited
element `font-size` now accepts bounded `vw`, `vh`, `vmin`, and `vmax` values
resolved against the validated configured viewport, with checked thousandths,
half-up integer-pixel conversion, content-wire carry, and out-of-range cascade
fallback. Full CSS variable grammar, dynamic viewport recomputation, and
complete CSS font-size parity remain issue #40 gates.

The completed CSS fractional-pixel `font-size` follow-up is
[`native-engine-browser-558`](tasks/native-engine-browser-558.md): inherited
element `font-size` now accepts bounded fractional `px` values, converts
thousandths with checked half-up rounding, and preserves the native 1 through
256 px result bound. Full CSS variable grammar, dynamic viewport recomputation,
and complete CSS font-size parity remain issue #40 gates.

The completed CSS root-relative `font-size` follow-up is
[`native-engine-browser-557`](tasks/native-engine-browser-557.md): inherited
element `font-size` now accepts bounded `rem` values resolved against the
computed root element size. Root-relative factors use the same checked
thousandths and half-up integer-pixel path as `em` and percentages, with
out-of-range results falling through to the existing cascade fallback. Full CSS
variable grammar, dynamic viewport recomputation, and complete CSS font-size
parity remain issue #40 gates.

The completed CSS relative `font-size` follow-up is
[`native-engine-browser-556`](tasks/native-engine-browser-556.md): inherited
element `font-size` now accepts bounded `em` and percentage values. Factors use
the existing thousandths grammar, scale the computed parent size with checked
half-up integer arithmetic, and preserve the native 1 through 256 px bound;
out-of-range relative results fall through to the existing cascade fallback.
Full CSS variable grammar, dynamic viewport recomputation, and complete CSS
font-size parity remain issue #40 gates.

The completed CSS absolute `font-size` follow-up is
[`native-engine-browser-555`](tasks/native-engine-browser-555.md): inherited
element `font-size` now accepts bounded CSS absolute units `pt`, `pc`, `in`,
`cm`, and `mm` alongside the existing integer `px` path. Values resolve
through the CSS 96 dpi reference pixel and round half-up to the native integer
pixel model, with the existing 1 through 256 px bound. Full CSS variable
grammar, dynamic viewport recomputation, and complete CSS font-size parity
remain issue #40 gates.

The completed FontFaceSet event-handler follow-up is
[`native-engine-browser-554`](tasks/native-engine-browser-554.md):
native `document.fonts` now exposes persistent `onloading`,
`onloadingdone`, and `onloadingerror` properties backed by the existing
EventTarget listener owner. Reassignment and null/non-callable clearing remove
the previous handler, and repeated host bootstrap preserves the properties.
Font-display timing, installed-font discovery, variable-axis completeness,
hinting, media output, and complete FontFace/Web IDL parity remain issue #40
gates.

The completed CSS FontFace display-carry follow-up is
[`native-engine-browser-553`](tasks/native-engine-browser-553.md):
`font-display` now survives static `FontFace` projection and identity/status
matching, dynamic `FontFaceInstall` commands, native font resources, and the
content-process wire. The document owner revalidates `auto`, `block`, `swap`,
`fallback`, and `optional`, with legacy missing fields defaulting to `auto`.
Font-display timing, block/swap/fallback periods, installed-font discovery,
variable-axis completeness, hinting, media output, and complete FontFace/Web
IDL parity remain issue #40 gates.

The completed CSS FontFace metric-override follow-up is
[`native-engine-browser-552`](tasks/native-engine-browser-552.md):
`ascent-override`, `descent-override`, and `line-gap-override` now accept
bounded `normal` or 0% through 1000% values in static `@font-face` rules and
dynamic page-realm `FontFace` descriptors. The normalized tenth-percent
values survive native resource/content-process wire construction, static
identity/status projection, and selected-face line metrics after `size-adjust`.
Invalid values fail closed before resource mutation. Font-display timing,
installed-font discovery, variable-axis completeness, hinting, media output,
and complete FontFace/Web IDL parity remain issue #40 gates.

The completed CSS FontFace `size-adjust` follow-up is
[`native-engine-browser-551`](tasks/native-engine-browser-551.md): bounded
`size-adjust` percentages from 25% through 400% now normalize in static
`@font-face` rules and dynamic page-realm `FontFace` descriptors. The value
survives native resource and content-process wire construction, participates
in static face identity/status matching, and scales selected-face metrics,
shaping, and rasterization. Font-display timing, installed-font discovery,
remaining CSS FontFace descriptors, and complete FontFace/Web IDL parity remain
issue #40 gates.

The completed CSS FontFace feature-default follow-up is
[`native-engine-browser-550`](tasks/native-engine-browser-550.md): bounded
`font-feature-settings` descriptors now normalize in `@font-face` rules,
survive native resource construction, and project through static
`document.fonts` `FontFace.featureSettings`. Matching element-authored feature
tags still override face defaults during shaping, and static identity/status
matching includes the feature list. Font-display timing, installed-font
discovery, remaining CSS FontFace descriptors, and complete FontFace/Web IDL
parity remain issue #40 gates.

The completed FontFace variant-application follow-up is
[`native-engine-browser-549`](tasks/native-engine-browser-549.md): normalized
dynamic and static-projected `FontFace.variant` values now travel through the
install command and document-owner admission. The native owner maps bounded
font-variant tokens to OpenType defaults, lets explicit face
`featureSettings` entries override matching variant tags, and preserves
element-level feature precedence during shaping. Invalid host variants fail
closed before font admission; display timing, installed-font discovery, CSS
`@font-face` feature descriptors, and complete FontFace/Web IDL parity remain
issue #40 gates.

The completed FontFace feature-application follow-up is
[`native-engine-browser-548`](tasks/native-engine-browser-548.md): normalized
dynamic `FontFace.featureSettings` values now travel through the install command,
document-owner admission, and content-process wire. Native faces retain bounded
OpenType defaults; element-authored feature tags override matching face defaults
while unmatched face defaults suppress automatic shaping defaults. Invalid host
metadata fails closed, and legacy wire snapshots default to an empty list.
CSS `@font-face` resources remain unchanged until their feature descriptors are
parsed separately; display timing, installed-font discovery, and complete
FontFace/Web IDL parity remain issue #40 gates.

The completed FontFace variant-descriptor follow-up is
[`native-engine-browser-547`](tasks/native-engine-browser-547.md): dynamic and
static-projected `FontFace` objects now validate bounded `variant` and
`featureSettings` values against the native font-variant and
font-feature-settings grammar. Valid values normalize deterministically,
duplicate feature tags use last-wins semantics, and invalid constructors or
setters fail transactionally. Native install wire behavior is unchanged;
display timing, installed-font discovery, and complete FontFace/Web IDL parity
remain issue #40 gates.

The completed palette-override follow-up is
[`native-engine-browser-540`](tasks/native-engine-browser-540.md): bounded
`override-colors` descriptors now resolve through document-local named
palettes, validate exact unique CPAL source colors, and recolor admitted COLR
solid and gradient paints. Malformed, ambiguous, over-budget, and
unidentifiable mappings fail closed to monochrome fallback without silently
substituting a nearest color.

The completed named-palette follow-up is
[`native-engine-browser-539`](tasks/native-engine-browser-539.md): bounded custom
names now flow through inherited `font-palette` and CSSOM, while
`@font-palette-values` stores document-local last-wins `base-palette` selections
for COLR/CPAL painting. Unregistered names fail closed to palette zero; valid
color overrides are covered by slice 540.

The completed CSS palette-keyword follow-up is
[`native-engine-browser-538`](tasks/native-engine-browser-538.md): inherited
`font-palette: normal|light|dark` now selects palette zero or the first
bounded CPAL palette advertising the requested light/dark background flag.
Malformed metadata and missing semantic palettes fail closed to palette zero;
named base palettes are now covered by slice 539.

The completed affine sweep-gradient follow-up is
[`native-engine-browser-536`](tasks/native-engine-browser-536.md): COLRv1
sweep paints retain their authored center and angle endpoints in gradient
space while carrying a bounded finite nonsingular affine transform through
paint composition, synthetic stretch, and glyph-local pixel mapping. Raster
samples inverse-map into gradient space before angle evaluation, so skew,
non-uniform scale, rotation, reflection, and translation preserve sweep
semantics without approximation. Invalid transforms fail closed; the remaining
font, timing, and Web IDL gates remain issue #40 work.

The completed transformed COLRv1 clip-mask follow-up is
[`native-engine-browser-535`](tasks/native-engine-browser-535.md): finite
nonsingular clip rectangles are transformed into bounded convex quadrilaterals
at paint time and retained as a small nested mask list. Existing supersampled
raster samples test every polygon, so translated, rotated, scaled, reflected,
and skewed clip rectangles intersect without an unbounded mask surface.
Singular, non-finite, and out-of-bound transforms fail closed; degenerate
finite rectangles produce empty masks rather than leaking pixels.

The completed affine radial-gradient follow-up is
[`native-engine-browser-534`](tasks/native-engine-browser-534.md): COLRv1
radial paints retain exact two-circle geometry and carry finite nonsingular
affine transforms through glyph-local pixel mapping and synthetic stretch.
Inverse-mapped samples produce ellipse gradients for skew, non-uniform scale,
rotation, reflection, and translation; singular or non-finite transforms fail
closed. The remaining font, palette, timing, and Web IDL gates stay explicit.

The completed three-point COLRv1 linear-gradient follow-up is
[`native-engine-browser-531`](tasks/native-engine-browser-531.md): `p0` and
`p1` define the color line and `p2` defines its projection direction. All
three points survive finite affine transforms, synthetic stretch, and
glyph-local mapping; degenerate triples fail closed and valid triples use the
existing extend modes and sorted stops. Sweep skew parity was a follow-up
completed by slice 536.

The completed transformed COLRv1 gradient follow-up is
[`native-engine-browser-530`](tasks/native-engine-browser-530.md): composed
finite affine transforms reach linear gradient geometry directly, while radial
and sweep gradients first admit conformal translation, rotation/uniform scale,
and reflection with radius and direction mapping. Slice 534 extends the radial
path to the broader affine ellipse transform, slice 535 transforms clip
rectangles into polygons, and slice 536 extends sweep sampling to the broader
affine transform.

The completed bounded COLRv1 gradient follow-up is
[`native-engine-browser-529`](tasks/native-engine-browser-529.md): linear,
radial, and sweep COLRv1 paints now admit finite coordinates, at most 16
bounded stops, and pad/repeat/reflect extension into compact glyph-local
descriptors sampled at raster time. Nested clip boxes intersect during
supersampled coverage; malformed stops, unsupported sweep transforms,
unsupported paint graphs, and unbalanced state fail closed to monochrome
fallback at this checkpoint, with later affine radial, clip, and sweep
follow-ups recorded above.

The completed bounded COLRv1 clip-provenance follow-up is
[`native-engine-browser-528`](tasks/native-engine-browser-528.md): each
admitted outline carries a bounded generation, and current-outline clips are
accepted without a redundant mask only when the painted outline matches every
active clip generation. Replaced outlines, clip creation without an outline,
and unbalanced paint state fail closed to monochrome fallback. The transform,
compositing, layer, shaping, spacing, and two-crate contracts remain unchanged.

The completed bounded COLRv1 transform/compositing follow-up is
[`native-engine-browser-527`](tasks/native-engine-browser-527.md): finite
affine transform stacks and `SourceOver`/`DestinationOver` solid-layer modes
now survive `ttf-parser`, native glyph coverage, and software surface
compositing. Current-outline clips are balanced and bounded; clip boxes,
gradients, other blend modes, malformed paint state, and non-finite transforms
fail closed to monochrome fallback. The two-crate boundary and existing
shaping/spacing contracts remain unchanged. Gradient parity, clip-box masks,
remaining blend modes, bitmap/SVG color fonts, palette selection, color axes,
hinting, font-display timing, and complete FontFace/Web IDL parity remain issue
#40 gates.

The completed COLR/CPAL color-glyph follow-up is
[`native-engine-browser-526`](tasks/native-engine-browser-526.md): bounded
solid palette layers now carry their colors from `ttf-parser` through native
glyph coverage into the software raster surface. The existing shaping,
spacing, clipping, hit-test, font-container, and two-crate contracts remain
unchanged; unsupported gradients, transforms, clips, composite layers, and
malformed color paints fail closed to the monochrome outline path. COLR
gradient/transform parity, CBDT/CBLC and SVG-in-font sources, palette
selection, color variation axes, hinting, font-display timing, and complete
FontFace/Web IDL parity remain issue #40 gates.

The completed WOFF2 font-admission follow-up is
[`native-engine-browser-525`](tasks/native-engine-browser-525.md): valid WOFF2
font sources now pass through bounded pure-Rust Brotli/container decoding into
the existing SFNT font owners, with malformed headers, oversized compressed or
decompressed streams, and invalid output rejected before admission. CSS
`@font-face` and script-created `FontFace` source lists recognize
`format("woff2")`, while existing WOFF1/SFNT behavior, source fallback,
content-process wire shape, and the two-crate boundary remain unchanged. Color
glyph tables, variable-axis completeness, hinting, font-display timing, and
complete FontFace/Web IDL parity remain issue #40 gates.

The completed numeric font-face range follow-up is
[`native-engine-browser-524`](tasks/native-engine-browser-524.md): absolute
numeric `font-weight` singletons and ascending ranges from 1 through 1000 now
survive CSS `@font-face` and script-created `FontFace` parsing, native font
resource admission, content-process serialization, legacy singleton wire
decoding, and range-aware face matching. The requested element weight still
drives advertised variable `wght` mapping, and singleton formatting and
computed-style behavior remain unchanged. Custom axes, hinting, color glyph
tables, font-display timing, and complete FontFace/Web IDL parity remain issue
#40 gates.

The completed relative-weight follow-up is
[`native-engine-browser-523`](tasks/native-engine-browser-523.md): CSS
`lighter` and `bolder` now resolve against the inherited computed weight using
the bounded CSS relative-weight bands, including numeric weights from 1 through
1000. The resolved absolute value feeds existing face scoring, advertised
`wght` mapping, paint selection, inheritance/CSS-wide resets, and CSSOM;
`@font-face` and script-created `FontFace` descriptors remain absolute-only for
relative keywords. Custom axes, hinting, color tables, and complete
FontFace/Web IDL parity remain issue #40 gates.

The completed variation-aware glyph-rasterization follow-up is
[`native-engine-browser-519`](tasks/native-engine-browser-519.md): non-default
variable-font coordinates now reach both the HarfRust shaping owner and the
native glyph bitmap. A bounded `ttf-parser` outline collector flattens
TrueType/CFF lines and curves and uses fixed 4x supersampling under explicit
point, dimension, and font-size limits; static/default-instance, bitmap-only,
and malformed outlines retain the fontdue fallback. Hinting, color glyph
tables, automatic `font-weight`/`font-stretch` axis mapping, and full
FontFace/Web IDL parity remain issue #40 gates.

The completed bounded FontFace variation-descriptor follow-up is
[`native-engine-browser-518`](tasks/native-engine-browser-518.md): the
`@font-face` and script-created `FontFace` `variationSettings` descriptors
now use the bounded quoted axis representation, cross the CSS/host/content
wire, project through the page FontFace surface, and provide defaults to the
native HarfRust face shaper. Authored element axes override matching face
defaults; fontdue contour rasterization, automatic axis mapping, color tables,
and complete FontFace/Web IDL parity remain issue #40 gates.

The completed bounded `font-variation-settings` follow-up is
[`native-engine-browser-517`](tasks/native-engine-browser-517.md): inherited
quoted four-byte OpenType axis/value pairs now cross native CSS cascade and
CSSOM into HarfRust `ShaperInstance` coordinates, with duplicate-axis
last-wins normalization, CSS-wide resets, bounded decimal serialization, and
atomic malformed-value rejection. The existing character fallback and
fontdue raster path retain their default instance; variable rasterization,
color tables, and complete FontFace/Web IDL parity remain issue #40 gates.

The completed bounded `font-language-override` follow-up is
[`native-engine-browser-516`](tasks/native-engine-browser-516.md): it carries
an inherited, fixed four-byte OpenType language-system tag through computed
CSSOM and the HarfRust shaping owner, including CSS-wide resets, exact padded
tag serialization, content-process defaults, and malformed-value rejection.
Full language negotiation, mixed-script and vertical shaping, and complete
CSS Fonts/Web IDL parity remain issue #40 gates.

The completed bounded `font-variant` shorthand follow-up is
[`native-engine-browser-515`](tasks/native-engine-browser-515.md): it expands
the existing font-variant groups with CSS-wide reset semantics, ordinary
cascade order, and canonical CSSOM serialization.

The completed bounded `font-variant-east-asian` follow-up is
[`native-engine-browser-514`](tasks/native-engine-browser-514.md): inherited
form, width, and ruby controls now cross native CSSOM and map to bounded
OpenType feature tags, with explicit low-level feature precedence.
Language-specific shaping and vertical-writing behavior remain separate
profile gates.

The completed bounded `font-variant-alternates` follow-up is
[`native-engine-browser-513`](tasks/native-engine-browser-513.md): inherited
`normal` and `historical-forms` values now cross native CSSOM and map the latter
to the unambiguous `hist` OpenType feature, with explicit low-level feature
precedence. Parameterized feature-value alternates remain rejected until the
native `@font-feature-values` registry exists.

The completed `font-variant-numeric` follow-up is
[`native-engine-browser-512`](tasks/native-engine-browser-512.md): inherited
numeric figure, spacing, fraction, ordinal, and slashed-zero controls now
cross native CSSOM and map to bounded HarfRust OpenType tags, with explicit
low-level feature settings retaining precedence. Character fallback remains
bounded; numeric feature-specific shaping, variable/color fonts,
font-display timing, mixed-script shaping, cross-realm FontFace projection,
and complete FontFace/Web IDL parity remain issue #40 gates.

The completed bounded XHR XML-document response slice is
[native-engine-browser-423](tasks/native-engine-browser-423.md): page XHR
recognizes XML MIME responses, exposes a strict detached XML `Document` from
`responseXML` and `responseType = "document"`, and preserves bounded
namespaces, comments, CDATA, processing instructions, doctype metadata,
lookup, ownership, read-only behavior, and serialization. HTML document
responses, non-UTF encodings, external entities, streaming XML, and complete
XML/Web IDL parity remain issue #40 gates.

The completed URLSearchParams value-filter slice is
[native-engine-browser-453](tasks/native-engine-browser-453.md): page and
worker `URLSearchParams.has(name, value)` now distinguish matching and
non-matching values, aligning both realms while preserving name-only lookup,
live URL synchronization, insertion order, and bounded limits. Complete URL
SearchParams Web IDL descriptor and encoding parity remain issue #40 gates.

The completed Blob object-URL slice is
[native-engine-browser-454](tasks/native-engine-browser-454.md): page and
worker `URL.createObjectURL()` retain bounded Blob bytes behind an origin-
labelled `blob:` URL, and native Fetch plus synchronous/asynchronous XHR serve
GET/HEAD reads from that registry. `revokeObjectURL()` removes the entry and
later reads fail. The completed top-level navigation follow-up is
[native-engine-browser-455](tasks/native-engine-browser-455.md): same-realm
page navigation snapshots a bounded Blob document, carries it through the
content-process worker when needed, derives the creator origin, and commits a
fresh document realm. At the 455 checkpoint, generic image, stylesheet, script,
and cross-realm object-URL subresources remained issue #40 gates. The completed
image follow-up is [native-engine-browser-456](tasks/native-engine-browser-456.md):
process-backed page mutations can load a runtime-verified Blob URL into
`<img>` and CSS background image consumers through the existing bounded image
decoder and event/paint path, without network fallback or HTTP cache reuse.
The completed script follow-up is
[`native-engine-browser-457`](tasks/native-engine-browser-457.md):
process-backed page mutations can load a runtime-verified Blob URL as a classic
external script, enforce the document CSP, content-type, integrity, size, and
UTF-8 gates, execute without network/cache fallback, and deliver the normal
script-target `load`/`error` event through stable DOM ownership. The
completed stylesheet follow-up is
[`native-engine-browser-458`](tasks/native-engine-browser-458.md):
process-backed dynamic stylesheet links retain bounded per-link resource state,
resolve runtime-owned Blob CSS through document CSP, MIME, size, UTF-8, and
SRI gates, rebuild the native stylesheet/background-source owner, and deliver
the link `load`/`error` event without HTTP/cache fallback. Media, popup/window,
local-inline dynamic consumers, and cross-realm object-URL consumers remain
issue #40 gates. The completed module-dependency
follow-up is [`native-engine-browser-459`](tasks/native-engine-browser-459.md):
process-backed page module scripts resolve absolute runtime-owned Blob imports
through the bounded module-graph owner, reusing CSP, Blob-origin, MIME, size,
UTF-8, deduplication, and graph-limit gates without HTTP/cache fallback.
Relative Blob-derived dependency naming and parser-created cross-realm Blob
modules remain separate issue #40 gates. The completed local dynamic-script
follow-up is [`native-engine-browser-460`](tasks/native-engine-browser-460.md):
fixture and other non-network documents can load a runtime-owned Blob URL as a
dynamic classic script through the local owner, preserving bounded MIME, size,
UTF-8, integrity, and load/error checks without a network path. The completed
local stylesheet follow-up is
[`native-engine-browser-461`](tasks/native-engine-browser-461.md): non-network
documents can attach a runtime-owned Blob stylesheet, retain bounded per-link
success/failure state, rebuild the native CSS/background-source owners, and
deliver `load`/`error` without HTTP/cache fallback. Media, popup/window, and
cross-realm object-URL consumers remain issue #40 gates. The completed local
image follow-up is
[`native-engine-browser-462`](tasks/native-engine-browser-462.md): non-network
documents can load a runtime-owned Blob URL for a dynamically attached `<img>`
and CSS background-image consumer through the bounded native decoder, retain
intrinsic dimensions and paint resources, and deliver the image `load`/`error`
event without network or HTTP-cache fallback. Media and cross-realm object-URL
consumers remain issue #40 gates. The completed popup/window follow-up is
[`native-engine-browser-463`](tasks/native-engine-browser-463.md): popup and
`WindowProxy` navigation requests carry a bounded runtime-owned Blob snapshot
through local and content-process browser-effect queues, bootstrap the target
from `about:blank`, and commit the Blob document without network or HTTP-cache
fallback. Media and cross-realm object-URL consumers remain issue #40 gates.
The completed cross-realm message follow-up is
[`native-engine-browser-464`](tasks/native-engine-browser-464.md): page-owned
Blob URLs embedded in structured messages are snapshotted into a bounded
typed envelope for dedicated/shared workers, and worker-owned Blob URLs use
the same envelope on their way back to the page. Both inline fixture and
HTTP(S) content-process paths install the destination registry before clone
decoding, without network/cache fallback; source revocation remains
independent. At that checkpoint, page-window, MessagePort, Service Worker
client messaging, media, and remaining browser/Web IDL conformance remained
issue #40 gates.

The completed page-window message follow-up is
[`native-engine-browser-465`](tasks/native-engine-browser-465.md): popup and
`WindowProxy` messages carry bounded Blob URL snapshots through local,
parked-target, frame, and HTTP(S) content-process routes, with destination
registry installation before clone decoding. Bidirectional inline and HTTP(S)
tests preserve origin and MIME behavior without network/cache fallback. The
same slice makes ordinary popup/window navigation context-safe by avoiding a
reentrant QuickJS registry lookup for non-Blob URLs. MessagePort was
subsequently closed by slice 466; Service Worker client messaging, media, and
remaining browser/Web IDL conformance remain issue #40 gates.

The completed Service Worker client-message follow-up is
[`native-engine-browser-467`](tasks/native-engine-browser-467.md): Service
Worker `Client.postMessage()` now carries bounded Blob URL snapshots through
the worker, content-process, and page event owners, installing the destination
registry before structured-clone decoding. HTTP(S) coverage fetches the
delivered Blob without network/cache fallback and preserves source-worker
revocation independence. Channel-message events retain the HTML default empty
`origin`; media and remaining browser/Web IDL conformance remain issue #40
gates.

The completed bounded media-resource follow-up is
[`native-engine-browser-468`](tasks/native-engine-browser-468.md): local
fixture and HTTP(S) content-process documents can resolve runtime-owned Blob
URLs selected by `<audio>` and `<video>`, enforce `media-src`/`default-src`,
validate bounded supported media payloads, expose WAV duration metadata, and
deliver selected-source readiness/error state plus load/error events through
the native DOM and event paths. Codec/decoder playback, static HTTP media
loading, and complete media/Web IDL parity remain issue #40 gates.

The completed allowed-file follow-up is
[`native-engine-browser-473`](tasks/native-engine-browser-473.md): native file
documents and media are available only below explicitly configured absolute
roots, with bounded canonical reads, relative file resolution, local media
metadata, and content-process root transfer/sandbox bindings. Network pages
cannot use this path to read local files. File-backed scripts, stylesheets,
images, fonts, workers, downloads, and complete file-origin/Web IDL parity
remain issue #40 gates.

The completed rooted-file-subresources follow-up is
[`native-engine-browser-474`](tasks/native-engine-browser-474.md): rooted file
documents can load bounded external CSS before layout, execute classic/module
file scripts, and decode file-backed `<img>` and CSS background images.
Dynamic file script, stylesheet, and image attachment shares the same
canonical root, integrity, size, event, and paint/resource owners. File fonts,
workers, downloads, CSS URL base parity, module dependency graphs, complete
file-origin semantics, and complete Web IDL parity remain issue #40 gates.

The completed rooted-file-module-graph follow-up is
[`native-engine-browser-475`](tasks/native-engine-browser-475.md): static and
dynamic file modules now prefetch bounded relative/absolute `file:` imports,
`export ... from` references, and literal dynamic imports through the existing
QuickJS module source map. Canonical roots, duplicate suppression, graph-entry
and aggregate-byte limits, and fail-closed handling for bare, network/data/blob,
credential-bearing, missing, oversized, and out-of-root dependencies are
enforced before evaluation. The page and worker URL facades also preserve
canonical `file:///` URLs. Import maps, non-literal dynamic imports, file
fonts/workers/downloads, complete file-origin semantics, and complete Web IDL
parity remain issue #40 gates.

The completed rooted-file-CSS-URL-base follow-up is
[`native-engine-browser-476`](tasks/native-engine-browser-476.md): static and
dynamic rooted file stylesheets now canonicalize relative `url(...)` tokens
against the stylesheet's own file URL before the existing CSS cascade and
background-image resource path. Nested stylesheet/image directories therefore
use the same allowed-root, integrity, size, and native decode/paint owners;
comments and quoted non-URL text are preserved, and non-file owners are
unchanged. File fonts, other CSS resource types, URL escape grammar, network
stylesheet URL-base parity, complete file-origin semantics, and complete Web
IDL parity remain issue #40 gates.

The completed rooted-file-CSS-import-graph follow-up is
[`native-engine-browser-477`](tasks/native-engine-browser-477.md): static and
dynamic rooted file stylesheets now expand bounded literal quoted or `url(...)`
`@import` dependencies at their cascade positions, with each source retaining
its own stylesheet URL base. Duplicate imports are admitted once, cycles
terminate, aggregate bytes and graph entries are bounded, and malformed,
missing, credential-bearing, unsupported, or out-of-root dependencies fail the
owning stylesheet closed. Media/layer/supports import conditions, escaped CSS
URL grammar, file fonts, other CSS resource types, network stylesheet URL-base
parity, complete file-origin semantics, and complete Web IDL parity remain
issue #40 gates.

The completed rooted-file-worker follow-up is
[`native-engine-browser-478`](tasks/native-engine-browser-478.md): file
documents can now create dedicated and shared classic or module workers with
credential-free rooted `file:` URLs. Existing bounded worker graph loaders
resolve relative `importScripts()` and module dependencies against each
worker's file URL while preserving isolated realms, message/error delivery,
and root/symlink/UTF-8/size checks; fixture and HTTP(S) workers are unchanged.
File-origin service workers, worklets, import maps, complete file-origin
semantics, and full worker Web IDL parity remain issue #40 gates.

The completed rooted-file-download follow-up is
[`native-engine-browser-479`](tasks/native-engine-browser-479.md): download
links owned by rooted file documents now use the configured canonical file
root, bounded download-byte reader, and existing queue/cancel/wait writer and
digest surface. Missing, directory, credential-bearing, and out-of-root file
targets fail closed; HTTP(S) downloads remain on their existing fetch and
navigation path. Content-Disposition precedence, resumable/background
downloads, file-origin service workers, and complete download/Web IDL parity
remain issue #40 gates.

The completed escaped-CSS-URL follow-up is
[`native-engine-browser-480`](tasks/native-engine-browser-480.md): rooted file
stylesheet `url(...)` and literal `@import` targets now decode bounded CSS
hex/simple escapes and line continuations before URL resolution, while escaped
closing parentheses remain inside unquoted tokens and invalid trailing escapes
fail closed. Non-file stylesheet owners retain their existing path.

The completed bounded-`@import`-conditions follow-up is
[`native-engine-browser-481`](tasks/native-engine-browser-481.md): rooted file
stylesheet graphs now retain and evaluate simple screen/all/print media
queries, viewport width/height and orientation features, bounded logical
`supports(...)` declaration conditions, and named or anonymous `layer`
preludes for both initial and dynamic stylesheets. Inactive imports are
skipped before dependency root admission, and active layered imports are
wrapped in the existing cascade-layer owner. Complex media features, full
CSS Supports/layer grammar, file fonts and other CSS resource types, network
stylesheet URL-base parity, complete file-origin semantics, and complete Web
IDL parity remain issue #40 gates.

The completed network stylesheet URL-base follow-up is
[`native-engine-browser-482`](tasks/native-engine-browser-482.md): direct
HTTP(S) stylesheet bodies now canonicalize relative CSS `url(...)` tokens
against the loaded stylesheet URL before the existing background-image and
resource loader paths. Rooted file sheets retain their file owner, and
data/blob/unsupported stylesheet owners remain byte-for-byte unchanged.
Redirect-final URL tracking, network `@import` fetching, file fonts and other
CSS resource types, complete file-origin semantics, and full Web IDL parity
remain issue #40 gates.

The completed redirected stylesheet URL-base follow-up is
[`native-engine-browser-483`](tasks/native-engine-browser-483.md): the
content-process loader now preserves the final HTTP(S) stylesheet URL with its
body across direct loads, cache hits, and 304 revalidation, so relative CSS
`url(...)` resources resolve against the redirected sheet while raw link
identity remains stable for dynamic mutations. Network CSS `@import` fetching,
file fonts and other CSS resource types, complete file-origin semantics, and
full Web IDL parity remain issue #40 gates.

The completed network CSS import follow-up is
[`native-engine-browser-484`](tasks/native-engine-browser-484.md): direct
HTTP(S) stylesheet graphs now fetch bounded recursive literal `@import`
dependencies through the existing HTTP policy/redirect/cache/MIME/integrity
owners, evaluate supported viewport conditions, preserve each dependency's
final URL base, and splice active rules at their import positions. Duplicate
and cyclic dependencies are suppressed within each graph, with bounded entry
and byte totals; file fonts and other CSS resource types, complete file-origin
semantics, and full Web IDL parity remain issue #40 gates.

The completed real-font metrics follow-up is
[`native-engine-browser-485`](tasks/native-engine-browser-485.md): bounded
inherited `font-family` lists and positive pixel `font-size` values now select
deterministic allowlisted system faces when available. Fontdue supplies real
advances, kerning, line metrics, and per-glyph coverage through immutable
`GlyphRun` display commands and the clipped software raster path; pages with
no explicit supported family or no available face retain the fixed-cell
fallback. `@font-face`, arbitrary font discovery, relative/non-pixel sizes,
variable fonts, complex shaping, grapheme-safe line breaking, bidi, and full
text-rendering parity remain issue #40 work.

The completed LTR shaping follow-up is
[`native-engine-browser-486`](tasks/native-engine-browser-486.md): explicit
supported system faces now retain HarfRust cluster order, glyph IDs, and
fractional advances, apply spacing at shaped cluster boundaries, and use
fontdue's indexed rasterization for ligatures and positioned marks. Whitespace
ranges continue to feed justification and decoration. A face that HarfRust
cannot parse, or a run with unsupported directionality, uses the bounded
character-by-character fontdue recovery path. Font fallback across missing
glyphs, direction/writing-mode propagation, full bidi and language/script
negotiation, `@font-face` resources, variable-font selection, grapheme-safe
line breaking, and complete text-rendering parity remain issue #40 gates.

The completed CSS-direction shaping follow-up is
[`native-engine-browser-487`](tasks/native-engine-browser-487.md): the
already-cascaded `direction:ltr|rtl` value now reaches native text metrics.
Horizontal RTL runs use HarfRust's reverse cluster order and mirrored
left-origin glyph coordinates, including mirrored whitespace ranges for
decoration and justification; LTR runs remain unchanged. The fixed-cell
fallback retains the direction for existing alignment behavior. Mixed bidi
segmentation, Unicode bidi reordering, vertical writing modes,
language/script negotiation, missing-glyph font fallback, and complete
text-rendering parity remain issue #40 gates.

The completed ordered font fallback follow-up is
[`native-engine-browser-488`](tasks/native-engine-browser-488.md): the cached
font book now retains the best matching face for each declared family. A run
covered by one face keeps HarfRust shaping; unsupported characters select the
first declared face with a real cmap glyph, and kerning is retained within
each selected face. Mixed-coverage runs use the bounded character recovery
path while preserving the existing indexed raster/display-list owners.
`@font-face` resources, arbitrary discovery, variable-font selection, and
complete text-rendering parity remain issue #40 gates.

The completed document-font follow-up is
[`native-engine-browser-489`](tasks/native-engine-browser-489.md): valid
named-family `@font-face` rules may admit bounded `data:` font sources after
MIME, decoding, size, and document CSP `font-src` checks. Admitted bytes are
transferred through content-process protocol v13 and placed ahead of system
faces in the document-local font book used by the existing HarfRust/fontdue
metrics and raster paths. File/network/blob sources, `local()` resolution,
font-loading events, variable/color fonts, and complete text-rendering parity
remain issue #40 gates.

The completed external document-font follow-up is
[`native-engine-browser-490`](tasks/native-engine-browser-490.md): rooted file
documents, runtime-owned Blob URLs, and HTTP(S) documents can admit bounded
font bytes through their existing file, object-URL, or network owners. HTTP
fonts enforce redirects, `font-src`, mixed-content, CORS, MIME, cookie,
referrer, and streamed byte limits before entering the document-local font
book; custom faces remain ahead of system candidates for the existing
HarfRust/fontdue paths. Font response caching, `local()` lookup,
font-loading events, variable/color fonts, and complete text-rendering parity
remain issue #40 gates.

The completed network-font-cache follow-up is
[`native-engine-browser-491`](tasks/native-engine-browser-491.md): HTTP(S)
`@font-face` responses now use a bounded cache partitioned by document origin,
requested URL, and same-origin cookie state. Fresh entries are reused only
after current policy checks; stale entries revalidate with conditional headers
and a 304 reuses the previous bounded bytes. `Set-Cookie`, `no-store`, cookie
or wildcard `Vary`, partial responses, missing cache metadata, and invalid or
oversized bodies are not retained. The cache is in-memory/process-local;
`local()` lookup, loading events, variable/color fonts, and complete
text-rendering parity remain issue #40 gates.

The completed data-media follow-up is
[`native-engine-browser-472`](tasks/native-engine-browser-472.md): static and
dynamic local media plus HTTP(S) content-process media now admit bounded
`data:` payloads through base64 or percent decoding, explicit MIME/sniffing,
and the existing policy/resource/event path. Embedded media never enters
network or cache transport. File URLs, decoder output, independent media task
source scheduling, range transport, and complete media/Web IDL parity remain
issue #40 gates.

The completed media timeline follow-up is
[`native-engine-browser-471`](tasks/native-engine-browser-471.md): finite
duration media now advances `currentTime` against the native monotonic clock,
records merged `played` intervals, rebases pause/resume, seek, and playback
rate changes, and reaches a paused `ended` state with one `ended` event.
`buffered`/`seekable` retain the full admitted range. Independent media task
source scheduling, decoded output, range transport, and complete media/Web IDL
parity remain issue #40 gates.

The completed bounded media play follow-up is
[`native-engine-browser-470`](tasks/native-engine-browser-470.md): admitted
finite-duration media now resolves `play()` and dispatches the `play` and
`playing` lifecycle transition; `pause()` returns the element to paused and
dispatches `pause`. Load errors, missing resources, and unknown-duration media
reject with `NotSupportedError`. Decoder output, playback-clock progression,
range-backed seeking, and complete media/Web IDL parity remain issue #40 gates.

The completed static HTTP media follow-up is
[`native-engine-browser-469`](tasks/native-engine-browser-469.md): initial and
dynamic HTTP(S) media loads use the native bounded GET owner, enforce
mixed-content, redirect, cookie, response MIME, streaming-size, and
`media-src`/`default-src` policy checks, and project supported metadata into
the same DOM/resource/event path. Decoder-backed playback, audio/video output,
richer timelines, and complete media/Web IDL parity remain issue #40 gates.

The completed MessagePort follow-up is
[`native-engine-browser-466`](tasks/native-engine-browser-466.md): page,
dedicated/shared-worker, popup/`WindowProxy`, and Service Worker MessagePort
delivery carry bounded Blob URL snapshots through local and HTTP(S)
content-process routes, installing the destination registry before clone
decoding. Bidirectional tests cover inline, content-process, popup, and
service-worker owners without network/cache fallback. Channel-message events
retain the HTML default empty `origin`; Service Worker client-message Blob
transfer, media, and remaining browser/Web IDL conformance remained issue #40
gates at that checkpoint. The client-message Blob transfer gate is now closed
by slice 467.

The completed report-only navigation preflight slice is
[native-engine-browser-413](tasks/native-engine-browser-413.md): the
process-backed child reports its live `navigate-to` observations through a
typed preflight before parent-owned top-level decisions, and the parent
dispatches them through the existing page event channel without duplicating
the first load observation. Enforced parent source snapshots remain active;
broader redirect and CSP certification remain open issue #40 gates.

The completed form-action report-delivery slice is
[native-engine-browser-414](tasks/native-engine-browser-414.md): the
content-process submit turn transfers report-only `form-action` violations
with its mutation envelope after submit handlers determine the final target;
the parent commits the snapshot and dispatches them through the persistent
page event bridge before navigation proceeds. Click and page-script submission
share the same child-owned policy ordering; enforced blocks still issue no
request, and report-only policy does not change authorization. Broader CSP and
Core Web Profile certification remain open issue #40 gates.

The completed form-target browsing-context slice is
[native-engine-browser-415](tasks/native-engine-browser-415.md): local and
HTTP(S) GET form submissions now preserve submitter `formtarget`/form
`target`, route `_blank` and named targets through the browser-owned target
effect queue, and route nested `_parent`/`_top` submissions to the correct
ancestor frame. Ancestor navigation promotes the navigated frame and removes
the replaced child selection. The following 416 slice carries the original
POST payload through those same effects instead of rejecting or downgrading
it; broader HTML target semantics remain issue #40 gates.

The completed form POST target payload slice is
[native-engine-browser-416](tasks/native-engine-browser-416.md): method, body,
and content type are carried through local and content-process frame,
popup, and named-target navigation effects. New POST targets must avoid a
duplicate GET, and the parent must validate the transferred payload against
the document-owned form state before dispatch. Multipart streaming and
broader form conformance remain separate issue #40 gates. Upload progress and
Service Worker body replay were completed by later 418 and 420 slices.

The completed compact binary form-body slice is
[native-engine-browser-417](tasks/native-engine-browser-417.md): multipart and
file-backed `Bytes` request bodies use compact base64 transport across the
content-process navigation envelope instead of a JSON number array. The
existing bounded body limit remains authoritative; upload streaming and
progress remain separate issue #40 gates.

The completed bounded XHR upload lifecycle slice is
[native-engine-browser-418](tasks/native-engine-browser-418.md): page and
worker XMLHttpRequest instances expose an upload event target and byte-accurate
buffered `loadstart`/`progress`/terminal/`loadend` delivery. Socket-level
chunk progress, streaming backpressure, and full ProgressEvent/Web IDL parity
remain separate issue #40 gates.

The completed page XHR response-lifecycle slice is
[native-engine-browser-419](tasks/native-engine-browser-419.md): successful
page XMLHttpRequest responses now publish `HEADERS_RECEIVED` and `LOADING`
between `OPENED` and `DONE`, with metadata available before bounded body
consumption. Incremental network chunks, synchronous XHR, streaming XML, and
full XHR/Web IDL parity remain separate issue #40 gates.

The completed Service Worker request-body replay slice is
[native-engine-browser-420](tasks/native-engine-browser-420.md): a controlled
POST is clonable, consumable through the original request, and replayable
through worker `fetch(Request)` with exact method, headers, content type, and
body bytes. The native dispatch restores the separately transferred content
type when constructing `event.request`; streaming replay and complete
Request/Streams Web IDL parity remain separate issue #40 gates.

The completed XHR JSON response-type slice is
[native-engine-browser-421](tasks/native-engine-browser-421.md): page and
worker asynchronous XMLHttpRequests accept `responseType = "json"` and expose
bounded parsed values through `response` while retaining an empty text
projection. Synchronous XHR, streaming JSON, and complete XHR/Web IDL parity
remain separate issue #40 gates.

The completed XHR ProgressEvent identity slice is
[native-engine-browser-422](tasks/native-engine-browser-422.md): page and
worker XHR upload lifecycle events are real `ProgressEvent` instances while
retaining bounded progress fields and upload-target identity. Socket-level
progress, complete event/Web IDL parity, and streaming upload remain separate
issue #40 gates.

The completed XHR XML-document response slice is
[native-engine-browser-423](tasks/native-engine-browser-423.md): page XHR
recognizes XML MIME responses, exposes strict bounded detached XML
`responseXML` and `responseType = "document"` behavior, and preserves XML
namespaces, node types, lookup, ownership, immutability, and serialization.
HTML document responses, non-UTF encodings, synchronous XHR, streaming XML,
and complete XML/Web IDL parity remain separate issue #40 gates.

The completed XHR response-type state slice is
[native-engine-browser-424](tasks/native-engine-browser-424.md): page and
worker XHR response types use canonical case-insensitive bounded setters, with
invalid values rejected at assignment and response-type changes rejected after
loading begins. Response projections, XML behavior, and lifecycle ordering
remain owned by the completed slices above.

The completed XHR HTML-document response slice is
[native-engine-browser-425](tasks/native-engine-browser-425.md): explicit
page `responseType = "document"` responses select a bounded detached
`text/html` document alongside the strict XML owner, with head/body/title
projections, case-insensitive lookup, basic HTML recovery, read-only
ownership, raw-text/RCDATA handling, and HTML serialization. Worker behavior,
non-UTF encodings, full HTML tree-builder semantics, and complete HTML/Web IDL
parity remain separate issue #40 gates.

The completed XHR `responseText` state slice is
[native-engine-browser-426](tasks/native-engine-browser-426.md): page and
worker XHR now use guarded internal projections so binary/JSON/document
response types reject `responseText` access, valid text responses remain
readable during buffered `LOADING`/`DONE`, and reopen/abort/terminal paths
clear stale text. Worker-XHR streaming and complete Web IDL parity remain
separate issue #40 gates.

The completed XHR download-progress slice is
[native-engine-browser-427](tasks/native-engine-browser-427.md): page and
worker XHR now dispatch one buffered response `ProgressEvent` while `LOADING`,
with truthful bounded byte counts, target identity, and validated
`Content-Length` handling before terminal delivery. Socket-level progress and
worker-XHR streaming remain separate issue #40 gates.

The completed page XHR streaming slice is
[native-engine-browser-428](tasks/native-engine-browser-428.md): page XHR
consumes the existing demand-driven native Fetch response stream and publishes
per-chunk `LOADING`/`progress` records, preserves split UTF-8 text, and
cancels the active reader on abort. At that checkpoint worker XHR remained
buffered through its separate whole-response Fetch host path; the
content-process worker streaming follow-up is recorded in slice 430. Streaming
upload, synchronous XHR, and complete XHR/Streams Web IDL parity remain issue
#40 gates.

The completed content-process worker XHR streaming slice is
[native-engine-browser-430](tasks/native-engine-browser-430.md): worker Fetch
responses now use the shared bounded demand-driven stream transport in the
content owner, and worker XHR publishes `HEADERS_RECEIVED`, per-chunk
`LOADING`/`ProgressEvent` updates, split-UTF-8-safe `responseText`, and
abort/reopen-safe terminal delivery. The inline worker follow-up is recorded
in slice 431; streaming upload, synchronous XHR, and complete XHR/Streams Web
IDL parity remain issue #40 gates.

The completed inline worker response-streaming slice is
[native-engine-browser-431](tasks/native-engine-browser-431.md): the inline
`NativeEngine` owner enables the worker stream registry, the fixture loader
returns bounded local response bodies through the shared demand-driven 8 KiB
transport, and the engine pumps those events at page boundaries. A real
worker witness covers two chunks split through a UTF-8 code point, response
cloning, body ownership, and total-byte preservation. HTTP(S) workers remain
owned by the content process. At that checkpoint page Fetch uploads,
synchronous XHR, and complete XHR/Streams Web IDL parity remained open; page
upload streaming is now covered by slice 432.

The completed page Fetch request-upload streaming slice is
[native-engine-browser-432](tasks/native-engine-browser-432.md): page
`ReadableStream` request bodies now cross the HTTP(S) content-process boundary
on one-shot transport demand, with bounded body bytes and chunk count,
cancel/error propagation, page callback mutation handling, and explicit
non-replayable redirect behavior. 301/302/303 method switches clear the body;
307/308 replay attempts reject. Fixture-owned bodyful requests, worker upload
bridges, controlled Service Worker upload replay, synchronous XHR, and complete
XHR/Streams Web IDL parity remain issue #40 gates.

The completed content-process worker Fetch request-upload streaming slice is
[native-engine-browser-433](tasks/native-engine-browser-433.md): dedicated and
SharedWorker `ReadableStream` request bodies now use the same one-shot,
demand-driven HTTP transport as page Fetch, including bounded bytes/chunks,
reader ownership, cancellation, worker teardown, and non-replayable redirect
handling. The buffered worker Fetch path remains unchanged; controlled Service
Worker upload replay, fixture-owned bodyful requests, synchronous XHR, and
complete XHR/Streams Web IDL parity remain issue #40 gates.

The completed Service Worker Fetch request-upload streaming slice is
[native-engine-browser-434](tasks/native-engine-browser-434.md): a
Service Worker-created `ReadableStream` passed to `fetch(Request)` now uses
the same bounded, one-shot HTTP upload transport, with demand-driven worker
pulls, cancellation/error propagation, and redirect replay rejection. The
existing buffered controlled-request replay remains covered; page-originated
controlled streaming interception, fixture-owned bodyful requests,
synchronous XHR, and complete XHR/Streams Web IDL parity remain issue #40
gates.

The completed fixture buffered-body slice is
[native-engine-browser-435](tasks/native-engine-browser-435.md): registered
fixture owners now accept bounded buffered POST/other body-bearing Fetch
requests while retaining the explicit fail-closed behavior for streaming
fixture bodies. This keeps deterministic fixtures useful for request-body
semantics without pretending they are a second network server; page-originated
controlled streaming interception, synchronous XHR, and complete
XHR/Streams Web IDL parity remained issue #40 gates at that checkpoint.

The completed controlled page stream-upload slice is
[native-engine-browser-436](tasks/native-engine-browser-436.md): a page
`ReadableStream` body now crosses the existing bounded FetchUpload demand
path, is materialized once, and reaches the active Service Worker FetchEvent
with its original request metadata. Handled responses resolve normally;
unhandled requests reuse the bounded body through the native HTTP loader, and
stream errors reject the page Fetch. Direct page/worker HTTP uploads remain
transport-demand-driven; fixture streaming bodies, synchronous XHR, and
complete XHR/Streams Web IDL parity remained issue #40 gates at that
checkpoint.

The completed fixture stream-upload slice is
[native-engine-browser-437](tasks/native-engine-browser-437.md): inline
dedicated and SharedWorker Fetch now consume bounded `ReadableStream` request
bodies through the worker upload-demand loop before invoking the deterministic
fixture loader with a replayable byte body. HTTP(S) workers retain their
direct transport stream; synchronous XHR and complete XHR/Streams Web IDL
parity remain issue #40 gates.

The completed synchronous-XHR slice is
[native-engine-browser-438](tasks/native-engine-browser-438.md): page and
dedicated/SharedWorker `XMLHttpRequest.open(..., false)` requests now reuse the
existing bounded fixture/HTTP(S) loader, request-body normalization, CORS,
cookie/cache/CSP state, and response projections. The host call runs on a
short-lived dedicated thread/runtime to preserve the blocking script-turn
contract without deadlocking the async owner; non-zero sync timeouts and
ReadableStream bodies fail closed. Responses are buffered and publish the
bounded terminal lifecycle, while complete XHR/Streams Web IDL parity remains
issue #40 work.

The completed XHR response-metadata slice is
[native-engine-browser-440](tasks/native-engine-browser-440.md): native HTTP
reason phrases now reach page and worker Fetch/XHR and synchronous-XHR
projections as `statusText`; the synthetic fixture response uses `OK`. Page
and worker response headers validate response names independently of request
restrictions, combine duplicate values, sort `getAllResponseHeaders()`
deterministically, and keep `Set-Cookie` unreadable. The loader remains the
CORS and response-policy owner; raw invalid wire-header bytes and complete
XHR/Web IDL descriptor parity remain issue #40 gates.

The completed XHR EventTarget identity slice is
[native-engine-browser-441](tasks/native-engine-browser-441.md): page and
dedicated/SharedWorker `XMLHttpRequest` and `XMLHttpRequestUpload` prototypes
inherit their realm's `EventTarget.prototype`, instances expose constructor
and `instanceof EventTarget` identity, and inherited listener/dispatch methods
drive the existing bounded lifecycle. Page handler attributes use the shared
owner-backed event store and worker handler attributes use the isolated worker
store; synthetic dispatch invokes both handler and listener callbacks. Full
descriptor and broader platform Web IDL parity remain issue #40 gates.

The completed XHR read-only-state slice is
[native-engine-browser-442](tasks/native-engine-browser-442.md): page and
dedicated/SharedWorker XHR expose non-enumerable prototype getters for
`readyState`, `status`, `statusText`, `responseURL`, `response`, and `upload`,
while internal lifecycle paths update private slots. Script mutation cannot
corrupt transport state; existing response-type, timeout, response-text, and
response-XML validators remain in force. Broader platform Web IDL descriptor
parity remains issue #40 work.

The completed XHR load-start lifecycle slice is
[native-engine-browser-443](tasks/native-engine-browser-443.md): page and
dedicated/SharedWorker XHR emit a zero-byte `loadstart` `ProgressEvent` on the
XHR target before upload progress and transport work, including synchronous
requests. Existing upload, response, cancellation, and terminal ordering
remain bounded and unchanged.

The completed XHR MIME-override slice is
[native-engine-browser-444](tasks/native-engine-browser-444.md): page and
dedicated/SharedWorker XHR expose bounded `overrideMimeType()` validation and
state errors, preserve an override across `open()`, apply it to page
XML/HTML-document projection and page/worker Blob MIME types, and retain the
actual wire `Content-Type` in response-header views. Async and synchronous XHR
share the same response-content ownership; full MIME-parameter parsing and
complete XHR/Web IDL parity remain issue #40 work.

The completed Fetch/Request URL-ownership slice is
[native-engine-browser-447](tasks/native-engine-browser-447.md): page and
dedicated/SharedWorker `Request` construction and Fetch dispatch now resolve
supported string, native `URL`, and existing native `Request` inputs against
their owning document or worker URL before host dispatch. Canonical absolute
URLs survive cloning and cross-owner transport, while the existing bounded
body/header/stream and loader policy owners remain authoritative. The inline
worker response-stream cached-body branch is restored so fixture responses use
the shared demand-driven 8 KiB transport. Complete URL/Fetch Web IDL parity
and broader method/scheme admission remain issue #40 work.

The completed canonical URL-resolution slice is
[native-engine-browser-448](tasks/native-engine-browser-448.md): page and
dedicated/SharedWorker URL construction and relative resolution now use the
existing bounded Rust URL owner before JavaScript projects URL fields. Special
schemes, default ports, escaping, and dot-segment joining therefore share one
canonical result across URL, Fetch, XHR, worker, and resource consumers.
Complete URL setter/Web IDL parity and the full browser scheme matrix remain
issue #40 work.

The completed worker URL-mutability slice is
[native-engine-browser-449](tasks/native-engine-browser-449.md):
dedicated/SharedWorker `URL` instances now expose bounded mutable accessors and
live `searchParams` synchronization, while worker `location` remains frozen
and read-only. Complete URL setter/Web IDL parity and the full browser scheme
matrix remain issue #40 work.

The completed page URL-setter normalization slice is
[native-engine-browser-450](tasks/native-engine-browser-450.md): page
`URL.pathname` and `URL.hash` writes now use the same bounded Rust canonicalizer
as initial and worker URL handling, preserving query state while escaping and
dot-normalizing setter values. Complete URL setter/Web IDL parity and the full
browser scheme matrix remain issue #40 work.

The completed general Fetch-method slice is
[native-engine-browser-451](tasks/native-engine-browser-451.md): page,
dedicated/SharedWorker, and Service Worker Fetch accept bounded valid HTTP
method tokens such as `REPORT` through one native owner, while CORS, body,
redirect, cache, and cookie policy remains bounded and navigation/form/XHR
method contracts stay separate. Complete Fetch Web IDL parity and the full
browser scheme matrix remain issue #40 work.

The completed general XHR-method slice is
[native-engine-browser-452](tasks/native-engine-browser-452.md): page and
worker XHR `open()` plus synchronous XHR now accept bounded valid HTTP method
tokens such as `REPORT` through the native loader, while XHR error categories,
body, timeout, and response-state rules remain explicit and navigation/form
methods stay separate. Complete XHR Web IDL parity and the full browser scheme
matrix remain issue #40 work.

The completed XHR realm-state mutator slice is
[native-engine-browser-445](tasks/native-engine-browser-445.md): page and
dedicated/SharedWorker XHR use Boolean-backed, state-gated `withCredentials`,
page synchronous XHR rejects `timeout` and nonempty `responseType` settings,
worker synchronous XHR permits those settings, and worker `document`
responseType assignment is ignored. Case-insensitive response-type canonical
values and bounded timeout validation remain active.

The completed XHR `open()` admission slice is
[native-engine-browser-446](tasks/native-engine-browser-446.md): page and
dedicated/SharedWorker XHR resolve relative URLs and URL objects against the
owning document/worker URL, apply the optional authority credentials
overload, use Boolean conversion for `async`, and return standards-shaped
method errors before cancelling an existing request. The existing bounded
method set remains explicit; broader HTTP methods, full URL parsing, and
complete XHR/Web IDL parity remain issue #40 work.

The completed XHR request-side slice is
[native-engine-browser-439](tasks/native-engine-browser-439.md): page and
worker XHR share bounded permitted request-header validation and duplicate
combination, expose ready-state constants on interfaces and prototypes, and
keep pending-send state reusable across validation failure and terminal
outcomes. The existing native loader remains the policy owner.

The completed page XHR reopen-cancellation slice is
[native-engine-browser-429](tasks/native-engine-browser-429.md): reopening a
live page XHR cancels its active response reader and stale stream
continuations cannot mutate the reused object or publish late progress and
terminal callbacks. At that checkpoint worker-XHR streaming was still open;
the content-process gate is now closed by slice 430, while the remaining
XHR/Streams Web IDL gates remain open.

The completed profile and viewport ownership slice is
[native-engine-browser-318](tasks/native-engine-browser-318.md). Native CLI
and TUI startup now share one configuration adapter: `--viewport` reaches the
native viewport contract, non-incognito profiles use Rust-owned storage under
`glass/native-profiles/<profile>/storage.json`, and `--incognito` keeps storage
volatile. Native profile state remains separate from Chromium state; persistent
session ownership and complete storage/profile conformance are still open
issue #40 gates.

The completed native snapshot ownership slice is
[native-engine-browser-319](tasks/native-engine-browser-319.md). CLI and MCP
snapshot creation now consume the native revisioned semantic observation and
persist the redacted result without opening a Chromium/CDP session. MCP native
startup also honors the selected native profile and incognito storage policy.
The completed native Task Protocol slice is
[native-engine-browser-320](tasks/native-engine-browser-320.md). CLI and MCP
task execution now share native revision guards, target preflight, confirmation
gates, form fill/read/validate/submit, navigation controls, bounded extraction,
dialog handling, pagination, and postcondition receipts. Workflow resume,
persistent session TUI/MCP multiplexing, richer native region/Web IR projection,
and complete profile certification remain open issue #40 gates. The completed
native persistent-owner slice is
[native-engine-browser-321](tasks/native-engine-browser-321.md): named native
sessions now own one in-memory runtime across CLI commands, with bounded
private-socket forwarding, fixed profile/storage/viewport policy, and explicit
native status/stop lifecycle semantics. The completed native persistent-surface
slice is [native-engine-browser-322](tasks/native-engine-browser-322.md): the
TUI can attach non-owningly to that same native owner, while MCP requests are
multiplexed into the owner's existing session rather than starting a second
engine. Revision-checked history and loading controls use the same owner
socket. Richer semantic/Web IR projection and final profile certification
remain active issue #40 work. The completed native
batch/workflow slice is
[native-engine-browser-323](tasks/native-engine-browser-323.md): typed batch
execution, declarative workflow state, bounded checkpoints, safe resume, and
TUI/owner workflow IPC now use the native runtime. Richer semantic/Web IR
projection and final profile certification remain active issue #40 work. The
completed native semantic observation slice is
[native-engine-browser-324](tasks/native-engine-browser-324.md): every native
CLI, persistent-owner, and MCP observation surface now exposes the shared
summary through raw levels, revision-checked region expansion, and bounded
policy-gated form values.

The completed native live Web IR slice is
[native-engine-browser-325](tasks/native-engine-browser-325.md): native
document, region, and frame observations now produce validated bounded Web IR
through the CLI and MCP `extractWebIr` surfaces, with explicit source
omissions, coverage, and output-budget metadata.

The completed resident native-first slice is
[native-engine-browser-326](tasks/native-engine-browser-326.md): ordinary
`glass-dev` browser starts now own a Glass-native runtime, while
`glass.browser.attach` is the explicit Chromium/CDP migration path. Resident
state, revision guards, workflows, screenshots, remote frames, and remote
inputs dispatch through the selected owner without silently crossing backends.
The completed native semantic MCP and stack-hardening slice is
[native-engine-browser-327](tasks/native-engine-browser-327.md): the shared
bootstrap, extraction, intent, knowledge, delta, checkpoint, diagnostics,
verification, wait, action, target, storage, and core browser contracts now
have explicit native resident routes. Heap-owned cascade scratch and a narrow
no-layout ready-state probe keep nested lifecycle/MCP calls stack-safe. The
remaining issue-40 work is complete operation coverage, Core Web Profile
conformance, recovery/cancellation, cross-platform release evidence, and
production certification.

The completed native capture-contract slice is
[native-engine-browser-328](tasks/native-engine-browser-328.md): automatic
backend selection includes native, and the shared capture operation exposes
native PNG and bounded PDF bytes without transport changes. JPEG and
screenshot-containing evidence remain explicit separate contracts.

The completed native recovery slice is
[native-engine-browser-329](tasks/native-engine-browser-329.md): native
content-owner recovery reloads the current URL under a revision guard,
replaces the current history entry, and never replays an indeterminate
mutation. Native certification is now `Partial`, not `Experimental`; Core Web
Profile conformance, process isolation, cross-platform evidence, and final
production certification remain active issue #40 gates.

The completed native visual capture slice is
[native-engine-browser-330](tasks/native-engine-browser-330.md): native PNG
screenshots now honor clip, scale, full-page, and semantic element options
through the shared CLI metadata contract. Full-page and off-screen element
captures use temporary raster geometry without changing live page state, and
nested frame composition is cropped only after child surfaces are aligned.
Native JPEG/WebP encoding and final cross-platform renderer certification
remain issue #40 promotion work.

The completed native content-worker liveness slice is
[native-engine-browser-331](tasks/native-engine-browser-331.md): dead content
workers are detected locally before new IPC, classified as typed failures, and
rebuilt by the explicit native recovery path. Navigation, script, action,
lifecycle, storage, and close boundaries share the same liveness refresh.
Process isolation, cancellation, conformance, and final production
certification remain active issue #40 gates.

The completed native startup Service Worker fetch-suspension slice is
[native-engine-browser-362](tasks/native-engine-browser-362.md): a restored
Service Worker can intercept the configured initial URL, await
`clients.openWindow()` across the content-process boundary, and keep the
initialization handoff alive while the browser owner creates the parked
target. Initialization enters the running state with the bounded effect
queued, and the normal scheduler then resolves the worker and commits the
resumed document. Browser-wide registration arbitration, durable live-client
leases, richer transferables, complete task-source scheduling, and final
production certification remain active issue #40 gates.

The completed native browser-profile Service Worker synchronization slice is
[native-engine-browser-363](tasks/native-engine-browser-363.md): running
active, parked, and frame-owned native targets refresh persisted registration
profiles before normal operations, reconcile removed or replaced worker
routes, and expose profile-backed registrations before lazy worker restoration.
In-memory registrations remain intact for no-storage sessions, and exact scope
boundary matching covers root, trailing-slash, and nested paths. Full
multi-instance registration arbitration, durable live-client leases, richer
transferables, complete task-source scheduling, and final production
certification remain active issue #40 gates.

The completed native durable Service Worker client-lease slice is
[native-engine-browser-364](tasks/native-engine-browser-364.md): bounded
profile sidecar leases preserve live window/frame client identities across
independent native sessions and content-process startup, prune crashed owners,
and use owner-fenced removal during orderly close. Active, parked, and frame
owners are merged with the durable projection before normal operations. Lease
heartbeats are operation-boundary updates with a bounded idle-expiry window;
full background event/task parity, multi-instance arbitration, and final
production certification remain active issue #40 gates.

The completed native Service Worker registration merge slice is
[native-engine-browser-365](tasks/native-engine-browser-365.md): content
owners journal final per-scope registration profiles and deletion tombstones,
and profile persistence applies those changes to the latest locked durable
vector. Concurrent owners changing independent scopes no longer erase one
another through stale whole-vector writes. The journal clears only after a
successful commit; same-scope conflicts are explicitly last-writer-wins at the
profile-lock boundary. Full cross-instance event ordering, background
task-source parity, and final production certification remain active issue #40
gates.

The completed native queued-transport host-turn slice is
[native-engine-browser-366](tasks/native-engine-browser-366.md): ordinary
content-process evaluations now admit callbacks already queued by open page
WebSocket or EventSource transports, even without a new transport command or
top-level `await`. The callbacks remain serialized through the existing
round-robin task-source turn and page mutation owner. This is not an autonomous
background loop; visibility/background scheduling, worker and Service Worker
fairness, full cross-source ordering, and final production certification remain
active issue #40 gates.

The completed native worker timer fairness slice is
[native-engine-browser-367](tasks/native-engine-browser-367.md): each content
or local host boundary runs at most one due dedicated/shared-worker timer turn
through a rotating worker-id cursor. Busy lower-id workers therefore cannot
monopolize successive page operations, while each worker realm keeps its
existing bounded timer queue and serialized command routing. Full
cross-source task ordering and final production certification remain active
issue #40 gates.

The completed native Service Worker timer admission slice is
[native-engine-browser-368](tasks/native-engine-browser-368.md): active and
waiting Service Worker realms share a rotating worker-id cursor, and each page
host boundary admits at most one due Service Worker timer turn. Timer-produced
client messages, MessagePort commands, and bounded Service Worker host work use
the existing settlement queues. This closes a worker-realm timer liveness gap
without claiming global cross-source ordering or final production
certification.

The completed native structured cross-realm event-dispatch slice is
[native-engine-browser-369](tasks/native-engine-browser-369.md): page-facing
Worker, MessagePort, and Service Worker client events now cross local and
content-process host turns as structured batches and enter already-installed
QuickJS dispatch functions directly. Valid 20,000-byte Worker messages no
longer consume the 16 KiB user-script source budget; per-message,
per-queue, transfer, and ordering bounds remain enforced. The following page
Fetch continuation slice removes the same source-size coupling from page
response settlement; complete global task-source arbitration remains an issue
#40 gate.

The completed native page Fetch response-dispatch slice is
[native-engine-browser-370](tasks/native-engine-browser-370.md): page Fetch
responses now enter the installed `__glassResolveFetch` function as bounded
parsed values, allowing a 20,000-byte HTTP response to resolve without
consuming the 16 KiB user-script source budget. Worker and Service Worker Fetch
response continuations, stream event transport, and complete global task-source
arbitration remain separate issue #40 gates.

The completed native Worker response-dispatch slice is
[native-engine-browser-371](tasks/native-engine-browser-371.md): dedicated,
shared, and Service Worker Fetch continuations, CacheStorage response
settlements, and `clients.openWindow()` settlements now enter installed worker
resolvers as bounded parsed values through one structured host-turn boundary.
20,000-byte Worker Fetch, Service Worker nested Fetch, and CacheStorage match
payloads cross the process without consuming the 16 KiB user-script source
budget; openWindow continuation coverage remains green. Page and worker stream
event transport remains a separate issue #40 gate.

The completed native network-event dispatch slice is
[native-engine-browser-372](tasks/native-engine-browser-372.md): page
WebSocket, EventSource, and Fetch-stream events plus dedicated/shared Worker
WebSocket and EventSource events now cross the host boundary as bounded parsed
values and call their installed dispatchers directly. Large page WebSocket,
EventSource, and Fetch-stream process witnesses, plus large Worker WebSocket
and EventSource witnesses, pass without interpolating payloads into
authored-source evaluation; existing task-source queues and transport limits
remain unchanged. Browser-wide task-source arbitration and the remaining Core
Web Profile gates remain active issue #40 work.

The completed native Service Worker transport-dispatch slice is
[native-engine-browser-373](tasks/native-engine-browser-373.md): Service
Worker Fetch request envelopes and lifecycle events now enter installed worker
callbacks as bounded parsed values and preserve their awaited Promise
settlement through a static continuation. Service Worker registration results
also use the page structured resolver. A process-backed 20,000-byte POST body
is consumed by `event.request.text()` and returned through `respondWith()`;
existing activation, interception, cache, client, and suspension tests remain
green. Browser-wide task-source arbitration and the remaining Core Web Profile
gates remain active issue #40 work.

The completed native Promise-rejection dispatch slice is
[native-engine-browser-374](tasks/native-engine-browser-374.md): unhandled and
handled Promise-rejection batches now enter the installed page dispatcher as
bounded parsed values instead of generated source. A five-event batch with
4,096-character reasons passes through the native runtime without consuming
the authored-script source budget; existing rejection event identity,
cancelability, ordering, and handler behavior remain green. Browser-wide
task-source arbitration and the remaining Core Web Profile gates remain
active issue #40 work.

The completed native page script-error dispatch slice is
[native-engine-browser-375](tasks/native-engine-browser-375.md): page script
error descriptors now enter the installed `ErrorEvent` dispatcher as bounded
parsed values rather than generated source. A five-error batch with
4,096-character messages passes without consuming the authored-script source
budget, while document continuation and existing error listener semantics
remain green. Browser-wide task-source arbitration and the remaining Core Web
Profile gates remain active issue #40 work.

The completed native cross-window message-dispatch slice is
[native-engine-browser-376](tasks/native-engine-browser-376.md): browser-owned
`postMessage` deliveries now cross local and content-process page turns as
structured page events and enter the installed `__glassDispatchMessage`
function directly. Local and HTTP(S) witnesses deliver 20,000-byte message
data without consuming the 16 KiB authored-script source budget; origin
filtering, WindowProxy source identity, queue bounds, and existing task-turn
ownership remain unchanged. Browser-wide task-source arbitration and the
remaining Core Web Profile gates remain active issue #40 work.

The completed native same-origin frame transport slice is
[native-engine-browser-377](tasks/native-engine-browser-377.md): frame DOM
commands and child-frame event metadata now cross the page-owner boundary as
typed `NativePageEventBatch` values and enter the installed frame dispatch or
command functions directly. A process-backed 16.3 KiB `innerHTML` mutation
passes even though the old generated source would exceed the 16 KiB authored
script budget; frame identity, command allowlists, event ordering, and the
existing bounded IPC/document limits remain enforced. Browser-wide task-source
arbitration and the remaining Core Web Profile gates remain active issue #40
work.

The completed native hash-change transport slice is
[native-engine-browser-378](tasks/native-engine-browser-378.md): local and
HTTP(S) fragment navigation now deliver old/new URL metadata through the typed
page-event batch and a static continuation. 8.2 KiB fragments cross both
owners without consuming the authored-script source budget, while
same-document navigation, URL validation, history behavior, and event re-entry
remain unchanged. Browser-wide task-source arbitration and the remaining Core
Web Profile gates remain active issue #40 work.

The completed native WindowProxy transport slice is
[native-engine-browser-379](tasks/native-engine-browser-379.md): browser-owned
WindowProxy target metadata now crosses the runtime boundary as validated
structured values and calls the installed `__glassSyncWindowProxies` dispatcher
directly. A maximum-size 16 KiB target URL is accepted and applied without
coupling the update to authored JavaScript source; popup identity, navigation,
close state, cache-key matching, content-process routing, and existing bounds
remain intact. Browser-wide task-source arbitration and the remaining Core Web
Profile gates remain active issue #40 work.

The completed native host-event transport slice is
[native-engine-browser-380](tasks/native-engine-browser-380.md): ordinary
focus, click, submit, keyboard, form, lifecycle, validation, image, and scroll
events now cross local and content-process page owners as bounded
`NativeHostEvent` records and call the installed dispatcher through a static
continuation. Default-prevention results for click, submit, keydown, and
beforeunload remain intact, while event metadata no longer consumes authored
JavaScript source budget. Browser-wide task-source arbitration and the
remaining Core Web Profile gates remain active issue #40 work.

The completed native SharedWorker connection transport slice is
[native-engine-browser-381](tasks/native-engine-browser-381.md): transferred
MessagePort descriptors now enter SharedWorker realms through the typed worker
dispatch channel and the installed connect dispatcher during a static
continuation. Named-worker reuse, connect ordering, port identity, and the
classic/module paths remain intact without a connection-specific generated
source string. Browser-wide task-source arbitration and the remaining Core Web
Profile gates remain active issue #40 work.

The completed native Service Worker client-projection slice is
[native-engine-browser-382](tasks/native-engine-browser-382.md): each Service
Worker turn now receives the bounded browser-wide client array through a
structured dispatcher before scripts, lifecycle callbacks, timers, messages,
or Fetch continuations run. Client identity, control, visibility, and
classic/module behavior remain intact without serializing live client state
into bootstrap source. Browser-wide task-source arbitration and the remaining
Core Web Profile gates remain active issue #40 work.

The completed native timer-probe transport slice is
[native-engine-browser-383](tasks/native-engine-browser-383.md): page and
worker timer-delay inspection now receives the host monotonic clock through a
direct numeric host value and executes fixed probe programs. Due-time ordering,
timer bounds, and the existing serialized owner remain unchanged; browser-wide
task-source arbitration and the remaining Core Web Profile gates remain active
issue #40 work.

The completed native structured-clone transport slice is
[native-engine-browser-384](tasks/native-engine-browser-384.md): native message
transport now carries a bounded tagged graph instead of a JSON-only value
projection. Local and cross-realm page/worker message paths preserve rich
cloneable values, cycles, shared object identity, binary views, Blob/File
metadata, and independent recipient clones; transferred MessagePort routing
retains its existing bridge contract. At that checkpoint, unsupported
functions, symbols, Promise-like values, SharedArrayBuffer, and ArrayBuffer
detachment remained explicit gates. Browser-wide task-source arbitration and
the remaining Core Web Profile gates remain active issue #40 work.

The completed native WindowProxy transfer-list slice is
[native-engine-browser-385](tasks/native-engine-browser-385.md): WindowProxy
`postMessage` now accepts both the legacy transfer-list overload and the
options-object overload, carries bounded MessagePort descriptors through page,
frame, and content-process delivery, and exposes decoded receiving ports in
`MessageEvent.ports` with structured-clone identity preserved. Source ports
remain detached only after a successful clone. Browser-wide task-source
arbitration and the remaining Core Web Profile gates remain active issue #40
work.

The completed native cross-target MessagePort slice is
[native-engine-browser-386](tasks/native-engine-browser-386.md): page-owned
port commands now cross the local and content-process boundaries into the
browser-owned target/frame route map, and a receiving port can transfer a new
port back to its original peer. Route ownership is bounded, insertion is
atomic, stale routes are pruned, and navigation/close paths clear old realm
routes before port IDs can be reused. Browser-wide task-source arbitration,
ArrayBuffer detachment, and the remaining Core Web Profile gates remain
active issue #40 work.

The completed native ArrayBuffer transfer slice is
[native-engine-browser-387](tasks/native-engine-browser-387.md): page, worker,
bridged MessagePort, WindowProxy, and content-process message paths now admit
bounded ArrayBuffer transfer lists, encode bytes and view aliasing before
commit, and detach sender buffers only after a successful clone. Detached and
duplicate members, typed-array/DataView transfer members, and
SharedArrayBuffer fail closed; `structuredClone(value, { transfer: [buffer] })`
returns a fresh clone and detaches its source. The existing JSON-safe wire
contract is unchanged. Complete transferability for other platform objects,
browser-wide task-source arbitration, and the remaining Core Web Profile gates
remain active issue #40 work.

The completed native same-realm transferable-port slice is
[native-engine-browser-388](tasks/native-engine-browser-388.md): local
`MessagePort.postMessage()` and `structuredClone(value, { transfer })` now
transfer bounded MessagePort and ArrayBuffer members through a fresh local
endpoint, move queued messages, preserve entanglement and data/port identity,
and invalidate the source only after successful decode. Cross-target ports
continue through the browser-owned route map, and invalid, duplicate,
detached, typed-array/DataView, and SharedArrayBuffer members fail closed. The
remaining transferable platform objects, browser-wide task-source arbitration,
and Core Web Profile gates remain active issue #40 work.

The completed native `ImageBitmap` transfer slice is
[native-engine-browser-389](tasks/native-engine-browser-389.md): the shared
structured-clone owner now transfers bounded pixel-backed `ImageBitmap`
instances through same-realm and page/worker message paths, reconstructs a
fresh receiver object, preserves dimensions and pixel readback, and closes the
source only after successful clone admission. Closed sources remain rejected,
and the existing message-size, origin, and canvas bounds remain enforced.
Other transferable platform objects, browser-wide task-source arbitration,
and the remaining Core Web Profile gates remain active issue #40 work.

The completed native `OffscreenCanvas` transfer slice is
[native-engine-browser-390](tasks/native-engine-browser-390.md): DOM-controlled
and standalone offscreen surfaces now transfer through the shared bounded
pixel descriptor, reconstruct a fresh receiver object, preserve worker 2D
painting and page raster readback, and invalidate both the transferred source
and its placeholder control after successful admission. Existing message,
canvas, origin, and payload limits remain enforced. Other transferable
platform objects, browser-wide task-source arbitration, and the remaining Core
Web Profile gates remain active issue #40 work.

The completed native `ReadableStream` transfer slice is
[native-engine-browser-391](tasks/native-engine-browser-391.md): page and
worker realms now transfer default and byte streams through a hidden,
demand-driven bridge rather than an eager snapshot. Source locking occurs
after clone admission, receiving reads issue bounded pull controls, and close,
error, cancellation, ordered chunks, and hidden transport ports use the
existing structured-clone/MessagePort owner. Local and page-to-worker
witnesses cover the transfer and bounded multi-turn scheduler. At that
checkpoint, worker-created underlying-source parity and remote-stream tee
parity remained open; the following 392 slice closes those two gaps. Upload
backpressure, browser-wide task-source ordering, and final Core Web Profile
certification remain active issue #40 work.

The completed native worker `ReadableStream` parity slice is
[native-engine-browser-392](tasks/native-engine-browser-392.md): worker-created
default and byte streams now drive bounded `start`/`pull`/`cancel` sources,
controllers, BYOB reads, closed/error settlement, and source locking. Worker
`tee()` now owns one upstream reader and bounded branch queues for both local
and transferred streams, with ordered close/error/cancellation propagation.
At that checkpoint, source-backed worker Request/Response bodies still stopped
at the synchronous snapshot boundary; the following 393 slice closes that
ownership gap while leaving true upload backpressure open.

The completed native worker stream-body slice is
[native-engine-browser-393](tasks/native-engine-browser-393.md): Worker
Request and Response objects retain source-backed, byte-mode, and transferred
streams; body methods drain them asynchronously through bounded consuming
owners; Request and Response cloning tees independent branches; and worker
Fetch drains both Request-source and options-body streams before dispatching
the existing host command. Direct stream disturbance now updates `bodyUsed`,
and aborts observed during a drain prevent dispatch. The host boundary still
buffers one bounded body payload, so chunked upload backpressure, worker
pipe/TransformStream parity, browser-wide task-source ordering, broader
transferables, and final Core Web Profile certification remain issue #40
gates.

The completed worker stream-composition slice is
[native-engine-browser-394](tasks/native-engine-browser-394.md): it adds the
worker `WritableStream` and `TransformStream` surfaces and wires worker
`ReadableStream.pipeTo()` and `pipeThrough()` through bounded in-realm queue,
lock, lifecycle, abort, cancel, and transformer ownership. Host streaming
backpressure, browser-wide task-source ordering, full Web IDL descriptors, and
final Core Web Profile certification remain separate issue #40 gates.

The completed native inline-content CSP slice is
[native-engine-browser-395](tasks/native-engine-browser-395.md): it moves
nonce/hash-preserving policy evaluation into the Rust owner for inline
classic/module scripts, `<style>` elements, and `style="..."` attributes,
including dynamic DOM mutation rechecks. The existing external resource policy
continues to use the shared URL matcher; broader CSP grammar and network
report delivery remain explicit issue #40 security gates.

The completed `script-src-attr` slice is
[native-engine-browser-396](tasks/native-engine-browser-396.md): it applies
the Rust-owned CSP fallback chain to inline `on*` content attributes and
reconciles admitted handlers across initial and dynamic DOM projections.

The completed scheduled-callback resilience slice is
[native-engine-browser-397](tasks/native-engine-browser-397.md): due timer,
animation-frame, and idle callbacks now report uncaught exceptions through the
existing page `error`/`onerror` surface and continue sibling callbacks in the
same bounded host turn. Autonomous rendering, background-page scheduling,
full task-source arbitration, and complete animation/idle Web IDL identity
remain issue #40 promotion gates.

The completed CSP report-only observation slice is
[native-engine-browser-399](tasks/native-engine-browser-399.md): response
`Content-Security-Policy-Report-Only` declarations preserve their original
policy text, bounded URL and inline checks deliver structured
`securitypolicyviolation` records, and synchronously inserted classic inline
scripts report through the persistent QuickJS page bridge without changing
authorization. Strict-dynamic, dynamic policy mutation, and network report
delivery remained explicit issue #40 security gates. The
completed CSP report-delivery lifecycle slice is
[native-engine-browser-400](tasks/native-engine-browser-400.md): Fetch drains
connect-policy records after the loader operation, EventSource carries
open/reconnect/error records to page and dedicated-worker owners, and
document-local inline-style observation reports a changed or newly attached
node once while rerunning enforced policy on every refresh. The records remain
bounded and report-only; strict-dynamic, dynamic
policy mutation, and the complete CSP source grammar remain issue #40
promotion gates.

The completed worker response-policy ownership slice is
[native-engine-browser-401](tasks/native-engine-browser-401.md): successful
HTTP(S) worker-script responses retain their enforced and report-only CSP under
the resolved worker URL, so dedicated and shared worker EventSource, Fetch, and
WebSocket owners use the worker policy while page `worker-src` authorization
remains distinct. Dynamic policy mutation, strict-dynamic, and the complete CSP
source grammar remain issue #40 promotion gates.

The completed CSP network report-delivery slice is
[native-engine-browser-403](tasks/native-engine-browser-403.md): report-only
`report-uri` declarations now emit bounded legacy CSP POST envelopes, while
`report-to` declarations resolve bounded groups from `Reporting-Endpoints` or
legacy `Report-To` response metadata and emit Reporting API envelopes. Report
endpoints are credential-free HTTP(S) targets resolved against the protected
document; secure documents reject insecure endpoints, report requests carry no
page cookies or redirects, and asynchronous delivery is capped so endpoint
failures cannot change the protected operation. Dynamic policy mutation,
strict-dynamic trust, and broader CSP conformance beyond the bounded source
matcher remain issue #40 promotion gates.

The completed CSP redirect path-matching slice is
[native-engine-browser-408](tasks/native-engine-browser-408.md): initial
Fetch, EventSource, stylesheet, image, script, worker, and embedded-frame
URLs remain path-aware, while each manually followed HTTP redirect ignores
only the host-source path through the shared matcher. Scheme, host, port,
mixed-content, credentials, and script nonce/strict-dynamic checks remain
enforced; report-only redirect observations use the same rule. Frame loading
retains the requested URL so an HTTP redirect is distinguished from a later
page-navigation handoff. Dynamic CSP grammar, report-only meta policy, and
broader conformance remain issue #40 promotion gates.

The completed Subresource Integrity slice is
[native-engine-browser-409](tasks/native-engine-browser-409.md): external
scripts and stylesheet links now parse strongest recognized SHA-256/384/512
metadata and verify raw response bytes before decoding, execution, or CSS
application. Fresh and `304` cache reuse are rechecked, parser and dynamic
scripts preserve `integrity`/`crossorigin`, and cross-origin integrity loads
require explicit CORS while bypassing URL-only cache reuse. Mismatches use the
existing resource-error path and leave the document alive. SRI for Fetch,
images, frames, media, Integrity-Policy headers, and future metadata options
remain separate issue #40 gates.

The completed CSP form-action slice is
[native-engine-browser-410](tasks/native-engine-browser-410.md): validated
GET/POST form submissions from local documents and the HTTP content process
now consult the shared `form-action` policy after submit handling and before a
navigation request is issued. Enforced policies intersect, and an omitted
`form-action` does not inherit `default-src`. The shared loader records
report-only observations; delivery through every form-event path and broader
browser conformance remain issue #40 gates.

The completed Glass `navigate-to` extension slice is
[native-engine-browser-411](tasks/native-engine-browser-411.md): bounded
response and initial head-meta navigation source groups cross the content
process boundary and are intersected by the parent navigation owner. Direct,
same-document, page-script, link, download, popup, history, and final
content-process navigation decisions consult the active policy before issuing
or committing a request. This is a Glass-owned extension rather than a
normative CSP Level 3 directive; live policy mutation was completed in the
following 412 transfer slice, while report-only delivery through every
navigation path was completed in the following 413 slice; form-action delivery
and broader browser conformance remain issue #40 gates.

The completed live `navigate-to` transfer slice is
[native-engine-browser-412](tasks/native-engine-browser-412.md): every
content-process mutation/evaluation that carries a document snapshot now
returns the child policy container's bounded effective source groups, and the
parent commits them before applying navigation or lifecycle effects. A CSP
meta policy inserted after load therefore governs the next link or
`location` navigation without requiring a reload. Report-only delivery through
every navigation path was completed in the following 413 slice; form-action
delivery and broader browser conformance remain issue #40 gates.

The completed CSP source-expression matching slice is
[native-engine-browser-404](tasks/native-engine-browser-404.md): the shared
native matcher now parses scheme sources and host sources with optional
schemes, exact or wildcard hosts, default/explicit/wildcard ports, and
percent-decoded slash-segment paths. It applies CSP secure scheme upgrades,
safe `'self'` upgrades, schemeless-host inheritance, and singleton-only
`'none'` semantics while rejecting malformed credentials, hosts, ports,
paths, and non-ASCII source syntax without widening access. Enforced and
report-only page/worker resources, frames, EventSource, and WebSocket policy
checks use the same owner. Dynamic policy mutation, strict-dynamic trust, and
broader CSP conformance remain issue #40 promotion gates.

The completed CSP strict-dynamic script slice is
[native-engine-browser-405](tasks/native-engine-browser-405.md): parser and
mutation-created page-script sources now preserve parser metadata and external
nonces through the content-process loader. Matching nonces authorize external
parser scripts; parser-inserted scripts without a matching nonce are blocked
when `strict-dynamic` is present; and non-parser-inserted external/module
scripts are admitted without a host allowlist. Enforced and report-only URL
checks share those decisions, inline script `unsafe-inline` cannot override a
strict-dynamic list, and initial lifecycle-created external scripts are no
longer dropped before loader handoff. Dynamic policy mutation and broader CSP
conformance remain issue #40 promotion gates.

The completed Service Worker document policy-container slice is
[native-engine-browser-407](tasks/native-engine-browser-407.md): controlled
page Fetch/XHR requests now pass the document's enforced `connect-src` and
mixed-content preflight before Service Worker evaluation, so a worker cannot
return a synthetic response around page policy. Handled requests retain
report-only ownership without duplicating unhandled fallback reports, and a
worker-served navigation's final-URL CSP is proven to govern the first page
Fetch. Dynamic policy mutation and broader CSP conformance remain issue #40
promotion gates.

The completed CSP meta-policy mutation slice is
[native-engine-browser-406](tasks/native-engine-browser-406.md): the native
document now records parser-processed enforced CSP meta elements and appends
new enforced CSP meta policies inserted into the live document head. The
policy container is additive, so removing a processed element or editing its
`content` attribute cannot relax earlier policy; every click, type, form, key,
lifecycle, hash-change, scroll, and script-mutation bridge commits newly
inserted policies before the next resource or inline-style decision. The
bounded policy cap and shared loader remain authoritative. Broader CSP
conformance remains a separate issue #40 promotion gate.

The completed WebSocket report-delivery slice is
[native-engine-browser-402](tasks/native-engine-browser-402.md): page and
dedicated-worker WebSocket `connect-src` checks carry bounded report-only
records across the asynchronous owner boundary and dispatch them before the
first open or handshake-error event. The actual `ws`/`wss` blocked URI remains
observable, normalized HTTP(S) policy matching avoids false reports, and
report-only records never alter enforced authorization. Dynamic policy
mutation, strict-dynamic, and the complete CSP source grammar remain issue #40
promotion gates.

The preceding CSP meta-composition slice is
[native-engine-browser-398](tasks/native-engine-browser-398.md): parser-time
enforced CSP policies in the document head now intersect with every response
header policy, and the same bounded policy applies to subresources, inline
elements/attributes, and child frames. Multiple frame source groups cross the
content-process boundary without being flattened into a permissive union.

The completed native visual-encoder slice is
[native-engine-browser-332](tasks/native-engine-browser-332.md): the native
visual capture contract now emits bounded PNG, JPEG, and WebP bytes with
quality handling, while preserving clip, scale, full-page, element, and
metadata behavior. MCP's native screenshot route now uses the same complete
capture contract as CLI and `BrowserSession`, including metadata. Process
isolation, cancellation, conformance, and final production certification
remain active issue #40 gates.

The completed native semantic-cookie-write slice is
[native-engine-browser-333](tasks/native-engine-browser-333.md): the shared
storage write operation now performs a current-origin `document.cookie`
assignment in native local and content-process documents, returns the resulting
cookie map, and uses the same semantics in the CDP adapter. Full cookie import
metadata remains available through `setCookies`; process isolation,
cancellation, conformance, and final production certification remain active
issue #40 gates.

The completed native message-channel slice is
[native-engine-browser-335](tasks/native-engine-browser-335.md): page and
dedicated-worker realms now expose bounded entangled `MessagePort` pairs and
same-realm `BroadcastChannel` delivery with cloned asynchronous messages,
stable `EventTarget`/`MessageEvent` identity, and typed limits. Cross-context
port transfer is covered by the following native transfer slice; shared
workers, richer transferable types, complete task-source scheduling, and final
production certification remain active issue #40 gates. Service workers are
covered by the following native owner slice.

The completed native service-worker slice is
[native-engine-browser-336](tasks/native-engine-browser-336.md): HTTP(S) pages
can register bounded classic or static-module workers, retain activated
registrations across reloads, expose registration/controller state, route
same-origin navigation and Fetch requests through longest-scope matching, and
return validated worker-generated responses without CDP. Durable worker caches,
complete task-source scheduling, and final production certification remain
active issue #40 gates.

The completed native cross-realm MessagePort slice is
[native-engine-browser-337](tasks/native-engine-browser-337.md): page and
dedicated-worker `MessagePort` endpoints can cross the native owner in both
directions, including worker-created ports returned to a page. Source
detachment, stable realm-qualified bridge identity, bounded transfer lists,
`MessageEvent.ports`, and replies are covered in both the in-process fixture
engine and HTTP(S) content process. Shared workers, richer transferable types,
complete task-source scheduling, and final production certification remain
active issue #40 gates. The follow-on
[native-engine-browser-338](tasks/native-engine-browser-338.md) slice extends
the same owner bridge to page-to-service-worker `MessagePort` transfer and
worker-to-page replies.

The completed native service-worker MessagePort slice is
[native-engine-browser-338](tasks/native-engine-browser-338.md): an active
same-origin service worker can receive a transferred page `MessagePort`,
deliver a ready event, and reply through that port across the in-process and
HTTP(S) content-process paths. Page-owned routes are cleared on navigation or
reload and removed when their worker is replaced or unregistered. Shared
workers, richer transferable types, complete task-source scheduling, and
final production certification remain active issue #40 gates. The completed
native SharedWorker ownership slice is
[native-engine-browser-339](tasks/native-engine-browser-339.md): local and
HTTP(S) pages can create named SharedWorkers, reuse one isolated worker
runtime for matching URL/name/type keys, attach independent connection ports,
receive `connect` events, and exchange bounded messages through the existing
owner bridge. Initial inline page creation is drained before the next local
turn so local and content-process startup have the same observable contract.
The completed native Service Worker CacheStorage slice is
[native-engine-browser-340](tasks/native-engine-browser-340.md): activated
HTTP(S) service workers own bounded exact-GET CacheStorage operations in Rust,
including `caches.open/delete/has/keys` and `Cache.match/put/delete/keys/add`
and `addAll`; lifecycle `waitUntil` work is settled before install, activate,
and fetch results are published; and cache entries are persisted in the
existing Rust storage profile and reloaded by a new content process. Cache
matching options were added in the later 343 slice; richer transferable
values, complete task-source scheduling, and final production certification
remain active issue #40 gates.

The completed native Service Worker registration persistence slice is
[native-engine-browser-341](tasks/native-engine-browser-341.md): canonical
same-origin registration metadata is stored with the profile, matching
registrations are restored before navigation and page-script execution, and
the existing worker script/module loading policy rebuilds the isolated worker
owner after a content-process restart. Full update/lifecycle conformance was
advanced by 342 and CacheStorage matching was completed by 343; richer
transferable values, complete task-source scheduling, and final production
certification remain active issue #40 gates.

The completed native Service Worker update slice is
[native-engine-browser-342](tasks/native-engine-browser-342.md):
`ServiceWorkerRegistration.update()` reloads the registered script through the
native resource policy, settles bounded install/activate work, replaces the
active isolated worker, refreshes persisted registration metadata, and routes
later navigation and Fetch requests through the new worker version. Waiting
and installing registration states, update notifications, byte-identical
short-circuiting, richer transferable values, complete task-source scheduling,
and final production certification remain active issue #40 gates. Bounded
document-navigation cache freshness/revalidation is covered by the later 346
slice.

The completed native Service Worker CacheStorage matching slice is
[native-engine-browser-343](tasks/native-engine-browser-343.md): bounded
`Cache.match()`, `Cache.delete()`, and `Cache.keys()` plus `caches.match()` now
implement `ignoreSearch`, `ignoreMethod`, and `ignoreVary`, retain request
headers for Vary comparisons, and return filtered request keys while preserving
the GET-only `Cache.put()` contract. The completed native Service Worker client
enumeration slice is
[native-engine-browser-344](tasks/native-engine-browser-344.md): an active
worker fetch now exposes a bounded current top-level window client through
`clients.matchAll()`, with type filtering, `includeUncontrolled`, stable opaque
identity, URL, frame, visibility, focus, and control metadata. Full
multi-client/frame lifecycle, `openWindow()`, richer transferable values,
complete task-source scheduling, and final production certification remain
active issue #40 gates. Bounded document-navigation cache
freshness/revalidation is covered by the following 346 slice. The completed native
Service Worker client-message slice is
[native-engine-browser-345](tasks/native-engine-browser-345.md):
`Client.postMessage()` now clones bounded data, carries existing MessagePort
transfers through the native owner, and delivers a ServiceWorker-container
`message` event to the current page at a bounded fetch/page turn.

The completed native document-cache freshness slice is
[native-engine-browser-346](tasks/native-engine-browser-346.md): native
document navigation now honors bounded `Cache-Control` freshness, retains
`ETag`/`Last-Modified` validators for stale entries, reuses validated `304`
responses, and evicts entries when `no-store`, cookie variance, or response
cookies make reuse unsafe. The pre-existing no-header session-cache behavior
is preserved; complete Service-Worker CacheStorage freshness, richer
transferable values, complete task-source scheduling, and final production
certification remain active issue #40 gates.

The completed native image-cache freshness slice is
[native-engine-browser-347](tasks/native-engine-browser-347.md): decoded
external images now honor bounded `Cache-Control` freshness, retain
`ETag`/`Last-Modified` validators for stale entries, reuse validated `304`
responses, and evict unsafe `no-store`/variance/cookie responses. Complete
Service-Worker CacheStorage freshness, richer
transferable values, complete task-source scheduling, and final production
certification remain active issue #40 gates.

The completed native stylesheet/page-script cache slice is
[native-engine-browser-348](tasks/native-engine-browser-348.md): HTTP(S) CSS
and classic page-script responses now use separate bounded freshness caches,
retain validators for stale entries, reuse validated `304` responses, and
evict unsafe entries. Worker source loading bypasses this page cache so worker
updates remain observable. Service-Worker CacheStorage freshness, richer
transferable values, complete task-source scheduling, and final production
certification remain active issue #40 gates.

The completed native Fetch/XHR cache slice is
[native-engine-browser-349](tasks/native-engine-browser-349.md): page Fetch,
XHR, and worker Fetch now share a bounded HTTP response cache partitioned by
owner origin, request identity, credentials, visibility, and cookie state.
`Request.cache` modes cover fresh reuse, forced reload, validator-driven
no-cache/304 reuse, force-cache, only-if-cached, and no-store isolation.
Responses without explicit cache metadata remain incremental streams;
Service-Worker CacheStorage freshness, richer transferables, complete
task-source scheduling, and final production certification remain active issue
#40 gates.

The completed native Service Worker lifecycle-observability slice is
[native-engine-browser-350](tasks/native-engine-browser-350.md): successful
registration and update operations now expose bounded installing, installed,
activating, and activated transitions through listener-bearing
`ServiceWorker`/`ServiceWorkerRegistration` objects, deliver `updatefound` and
`statechange`, resolve `registration.update()` with `undefined`, mark the old
active worker redundant after replacement activation, and remove a confirmed
unregistered registration from the page-facing map. Persistent waiting-worker
arbitration, exact `skipWaiting()` activation policy, multi-client controller
ownership, Service-Worker CacheStorage freshness, richer transferables,
complete task-source scheduling, and final production certification remain
active issue #40 gates.

The completed native Service Worker waiting-worker arbitration slice is
[native-engine-browser-351](tasks/native-engine-browser-351.md): workers that
do not request `skipWaiting()` remain installed and waiting beside the active
worker, with page-facing `registration.waiting` and installing/installed
events. Validated `skipWaiting()` requests still activate immediately, and a
matching native navigation promotes a waiting worker only after activate
`waitUntil()` work settles; failed promotion preserves the previous active
worker. Waiting-worker persistence, multi-client controller ownership, exact
task-source scheduling, Service-Worker CacheStorage freshness, richer
transferables, and final production certification remain active issue #40
gates.

The completed native Service Worker waiting-worker restoration slice is
[native-engine-browser-352](tasks/native-engine-browser-352.md): an installed
waiting worker's bounded script/type descriptor now persists beside the active
registration, both isolated worker realms restore through the native resource
policy, and a restored waiting worker remains visible until matching
navigation promotes it after activation work settles. Multi-client controller
ownership, exact task-source scheduling, Service-Worker CacheStorage
freshness, richer transferables, and final production certification remain
active issue #40 gates.

The completed native Service Worker client-control slice is
[native-engine-browser-353](tasks/native-engine-browser-353.md): the native
owner now tracks an explicit bounded top-level client identity and controller
scope, separates navigation interception from ordinary controlled-client
Fetch/XHR routing, honors `clients.claim()` after activation, and exposes
controller state, `controllerchange`, `clients.matchAll()`, and
`FetchEvent.clientId` consistently. Multi-client/tab/frame ownership, exact
task-source scheduling, Service-Worker CacheStorage freshness, richer
transferables, and final production certification remain active issue #40
gates.

The completed native Service Worker multi-client topology slice is
[native-engine-browser-354](tasks/native-engine-browser-354.md): the native
backend now derives stable context/frame client identities, reconciles known
target and nested-frame owners, and synchronizes bounded URL, type, frame,
visibility, and focus metadata into every live content process. Same-origin
`clients.matchAll()` honors `type` and `includeUncontrolled` across shared
profile-backed targets, and lifecycle, message, cache, and fetch turns consume
the same worker client projection. Cross-process client messaging,
`clients.openWindow()`, browser-wide registration arbitration, exact
task-source scheduling, durable live-client leases, and final production
certification remain active issue #40 gates.

The completed native Service Worker Cache API conformance slice is
[native-engine-browser-355](tasks/native-engine-browser-355.md): `Cache.put()`
now rejects error, opaque, redirect-opaque, partial (`206`), and `Vary: *`
responses, while bounded `Cache.addAll()` fetches through the shared worker
resource policy and commits all entries atomically. Install, activate, and
message turns can resume worker-owned Fetch commands through the same policy,
so Cache API operations are not limited to page-triggered fetches. This is
explicit CacheStorage state, not HTTP freshness: `Cache-Control` does not
silently evict application cache entries, and HTTP response freshness remains
owned by the resource-loader cache. Cross-process client messaging,
`clients.openWindow()`, browser-wide registration arbitration, worker-owned
task queues, durable live-client leases, and final production certification
remain active issue #40 gates.

The completed native networking task-order slice is
[native-engine-browser-356](tasks/native-engine-browser-356.md): page Fetch
commands now resolve in script-emission order through a FIFO networking queue,
and each response handoff returns to the persistent JavaScript owner before
the next queued response is processed. This removes the prior reverse-order
artifact for concurrent Fetch calls while cross-source task arbitration,
durable task queues, and final production certification remain active issue
#40 gates.

The completed native content task-source scheduling slice is
[native-engine-browser-357](tasks/native-engine-browser-357.md): one ready
host task is selected per cycle across Networking, WebSocket, FetchStream,
EventSource, and Timer using a rotating round-robin cursor. Networking keeps
its script-emission FIFO order, while due timers cannot be starved by
continuously ready Fetch work. The timer pump is suspended during host
continuations so explicit Timer turns own timer execution. Worker,
ServiceWorker, MessagePort, rendering, navigation-task queues, and final
production certification remain active issue #40 gates.

The completed native browser-effect arbitration slice is
[native-engine-browser-358](tasks/native-engine-browser-358.md): popup,
postMessage, navigation, and close effects now share a persistent round-robin
cursor, preserve FIFO order within each source, and re-enqueue nested effects
without allowing popup cascades to starve other browser-owned effects. This is
an operation-boundary scheduler; worker/ServiceWorker/MessagePort queues,
registration arbitration, live-client leases, and final production
certification remain active issue #40 gates.

The completed native Service Worker `clients.openWindow()` slice is
[native-engine-browser-359](tasks/native-engine-browser-359.md): a controlled
non-fetch Service Worker turn can request a validated same-origin HTTP(S)
window, the browser backend creates a real parked target with the source as
opener, and the persistent worker Promise resolves with the new WindowClient
descriptor. Requests cross the content-process boundary with stable IDs and
return through the same browser-effect scheduler, including nested effects.
Fetch-event suspension, browser-wide registration arbitration, durable
live-client leases, and final production certification remain active issue #40
gates.

The completed native Service Worker cross-target client-message slice is
[native-engine-browser-360](tasks/native-engine-browser-360.md):
`WindowClient.postMessage()` records addressed to another client now cross the
content-process boundary as bounded typed effects, resolve the opaque client id
against the exact selected or parked target/frame owner, and dispatch the page
`ServiceWorkerContainer` `MessageEvent` without changing selection. The
browser-owned scheduler includes client messages as a fair source and retains
same-client delivery behavior. Fetch-event suspension, browser-wide
registration arbitration, durable live-client leases, richer transferables,
complete task-source scheduling, and final production certification remain
active issue #40 gates.

The completed native Service Worker fetch-navigation suspension slice is
[native-engine-browser-361](tasks/native-engine-browser-361.md): a controlled
fetch event can await `clients.openWindow()` across the content-process IPC
boundary. The browser owner materializes the parked target, resolves the
worker Promise with its `WindowClient`, and resumes the retained fetch
continuation with either the Service Worker response or a bounded network
fallback. The navigation result is taken after the browser-owned effect
cascade. Browser-wide registration arbitration, durable live-client leases,
richer transferables, complete task-source scheduling, and final production
certification remain active issue #40 gates.

The completed native cookie-policy slice is
[native-engine-browser-334](tasks/native-engine-browser-334.md): cookie
profiles and `Set-Cookie` parsing now preserve SameSite and priority metadata,
reject insecure `SameSite=None`, default omitted SameSite to Lax, and filter
cookies using the request initiator across native navigation, Fetch/XHR,
EventSource, WebSocket, CSS, image, script, and worker requests. The bounded
schemeful-site owner is shared by these request classes; public-suffix-list,
partitioned-cookie, process isolation, cancellation, conformance, and final
production certification remain active issue #40 gates.

The completed computed-style/media-query slice is
[native-engine-browser-282](tasks/native-engine-browser-282.md). It exposes a
bounded read-only `getComputedStyle` surface backed by the native cascade and
layout snapshot, including common box, color, text, flex, overflow, and
geometry values, plus bounded `matchMedia` width/height/orientation queries.
Inline style mutations are reflected immediately and the style snapshot is
refreshed with the existing realm update path. This is an incremental page
runtime capability; full CSSOM, pseudo-elements, dynamic viewport-change
media delivery, and the wider native parity and production-promotion gates
remain open on issue #40.

The completed dedicated-worker slice is
[native-engine-browser-283](tasks/native-engine-browser-283.md). It adds
classic page-created `Worker` support for local fixtures and HTTP(S)
content-process pages, including isolated QuickJS realms, shared resource and
`worker-src` policy loading, bounded JSON-framed tagged structured-clone
message/error delivery,
startup/runtime failure events, and terminate/close ownership. Worker events
are delivered at explicit native page turns so local and content-process
execution remain deterministic. Shared/service workers, transferables, and
full worker-side network/Web IDL parity remain open on issue #40; bounded
worker timers, static worker import graphs, worker Fetch, and module dedicated
workers are covered by slices 284 through 291 below.

The completed dedicated-worker timer slice is
[native-engine-browser-284](tasks/native-engine-browser-284.md). It adds
bounded `setTimeout`, `setInterval`, cancellation, and worker-local
`performance.now()` scheduling to the isolated dedicated-worker realms. Due
worker callbacks are pumped before the next page evaluation in both the local
owner and HTTP(S) content process, and their messages/lifecycle effects use the
existing bounded worker queue. Continuous task-source fairness, automatic
delivery while a page is awaiting unrelated work, and complete worker timer
Web IDL semantics remain open on issue #40.

The completed dedicated-worker dependency slice is
[native-engine-browser-285](tasks/native-engine-browser-285.md). It preloads
bounded static `importScripts()` dependency graphs through the worker resource
and policy owner, executes dependencies before the root script, preserves
per-call consumption so unknown or dynamically resolved URLs fail explicitly,
and covers nested dependencies in the isolated worker realm. Dynamic expression
resolution, exact browser call-position semantics, module/shared/service
workers, full worker Fetch/XHR/streaming parity, and the wider native parity
gates remain open on issue #40.

The completed native-engine-browser-286 slice is
[native-engine-browser-286](tasks/native-engine-browser-286.md). It routes
classic-worker `fetch()` through the shared HTTP(S) resource, cookie, CORS,
redirect, and policy loader, with owner-tagged requests and bounded response
metadata plus text/JSON body consumers resolved inside the isolated worker
promise realm. Startup, message, and timer-created requests use the same
serialized host boundary. The follow-up 287 slice preserves binary request and
response bytes and adds bounded worker `bytes()`, `arrayBuffer()`, and `blob()`
consumers with clone/body ownership.

The completed native-engine-browser-287 slice is
[native-engine-browser-287](tasks/native-engine-browser-287.md). It carries
bounded raw response bytes through the worker Fetch handoff, accepts string,
`ArrayBuffer`, typed-array, Blob, and File request bodies without UTF-8
corruption, and exposes one-shot byte/ArrayBuffer/Blob response consumers on
isolated worker responses and clones. The follow-up 288 slice adds a bounded
worker `Response.body` stream with default/BYOB readers, cancellation, and
async iteration.

The completed native-engine-browser-288 slice is
[native-engine-browser-288](tasks/native-engine-browser-288.md). It exposes
the worker Fetch response body as a byte-preserving `ReadableStream` over the
host-buffered response snapshot, with reader locking, body disturbance,
bounded cancellation, BYOB reads, async iteration, and independent cloned
responses. Transport-demand streaming, full Request/Response Web IDL
identity, XHR, module/shared service workers, and the wider native parity
gates remain open on issue #40.

The completed native-engine-browser-289 slice is
[native-engine-browser-289](tasks/native-engine-browser-289.md). It adds
worker-realm `Headers`, `Request`, and `Response` constructor identity,
bounded mutable request headers, read-only response headers, request/response
clones, and one-shot body ownership across text, JSON, bytes, ArrayBuffer,
Blob, and stream consumers. The worker Fetch transport remains host-buffered;
XHR, module/shared/service workers, complete Web IDL parity, and the wider
native replacement gates remain open on issue #40.

The completed native-engine-browser-290 slice is
[native-engine-browser-290](tasks/native-engine-browser-290.md). It adds
asynchronous worker XMLHttpRequest with stable constructor identity, ready-state
transitions, request headers and bodies, text/ArrayBuffer/Blob responses,
response-header lookup, abort/timeout handling, and load/error events, all
routed through the existing worker Fetch owner. Upload progress, transport
cancellation, synchronous/XML XHR, shared/service workers, and the wider native
replacement gates remain open on issue #40.

The completed native-engine-browser-291 slice is
[native-engine-browser-291](tasks/native-engine-browser-291.md). It adds
dedicated module workers with bounded static and literal dynamic-import graph
prefetching, QuickJS module evaluation, module-worker `importScripts()`
semantics, and message/timer/network turns on the existing isolated worker
boundary. Shared/service/worklet workers, import maps, transferables, complete
worker Web IDL parity, and the wider native replacement gates remain open on
issue #40.

The completed native-engine-browser-292 slice is
[native-engine-browser-292](tasks/native-engine-browser-292.md). It adds a
worker-owned WebSocket bridge in the HTTP(S) content process: worker commands
are owner-tagged, persistent connections use the existing bounded WebSocket
transport and resource-policy target validation, and open/message/error/close
events plus text/binary sends cross the isolated worker boundary. Ping/pong
and clean close are covered end to end. The native local fixture owner reports
an explicit process-backed-network requirement rather than dropping a worker
connection command; shared/service/worklet workers, automatic background task
delivery, and the wider native replacement gates remain open on issue #40.

The completed native-engine-browser-293 slice is
[native-engine-browser-293](tasks/native-engine-browser-293.md). It adds
dedicated-worker EventSource/SSE through the shared HTTP(S) event-stream
transport, including relative worker resource URLs, open/error/close and
named-message dispatch, multiline data, `lastEventId`, origin, reconnect
control, cookie-change propagation, and owner-tagged close commands. Shared,
service, and worklet workers, automatic background task delivery, complete
worker Web IDL parity, and the wider native replacement gates remain open on
issue #40.

The completed native-engine-browser-294 slice is
[native-engine-browser-294](tasks/native-engine-browser-294.md). It adds the
worker runtime identity ordinary scripts expect: bounded `URL` parsing and
relative resolution, `URLSearchParams` construction/mutation/iteration, a
structured worker `location`, and a stable bounded `navigator` snapshot. The
local isolated-worker contract covers query decoding/encoding, duplicate
parameters, sorting, host/hostname/port decomposition, and worker-global
identity. Full URL and Web IDL conformance, live URL/search-parameter
synchronization, shared/service/worklet workers, automatic background task
delivery, and the wider native replacement gates remain open on issue #40.

The completed native-engine-browser-295 slice is
[native-engine-browser-295](tasks/native-engine-browser-295.md). It adds
worker `AbortController`/`AbortSignal` identity, abort listeners, abort
reasons, `AbortSignal.abort()`, bounded `timeout()` and `any()` composition,
and signal-aware worker Fetch rejection with late-result suppression. The
shared loader remains the transport/security owner; host transport
cancellation, full abort-event Web IDL behavior, and the wider native
replacement gates remain open on issue #40.

The completed native-engine-browser-296 slice is
[native-engine-browser-296](tasks/native-engine-browser-296.md). It adds
worker-standard runtime primitives for UTF-8 `TextEncoder`/`TextDecoder`,
bounded `atob`/`btoa`, `structuredClone`, `queueMicrotask`, `DOMException`,
`EventTarget`, `Event`, `CustomEvent`, `MessageEvent`, and `ErrorEvent`. The
native replacement gates remain open for transferable cross-realm ports,
complete Web IDL descriptors, and the remaining browser platform surface.

The completed native-engine-browser-297 slice is
[native-engine-browser-297](tasks/native-engine-browser-297.md). It adds an
OS-seeded worker `crypto` surface with bounded integer-typed-array
`getRandomValues()` and UUID v4 `randomUUID()`, including explicit quota and
type validation. Web Crypto `subtle` operations, cross-realm key transfer,
and the remaining native replacement gates remain open on issue #40.

The completed native-engine-browser-298 slice is
[native-engine-browser-298](tasks/native-engine-browser-298.md). It adds the
matching OS-seeded page `crypto` surface with bounded integer-typed-array
`getRandomValues()` and UUID v4 `randomUUID()`, preserving the realm object
across bootstrap re-entry. Web Crypto `subtle` operations, key objects,
cross-realm transfer, pool replenishment, and the remaining native replacement
gates remain open on issue #40.

The completed native-engine-browser-300 crypto-replenishment slice
keeps a bounded page/worker fast-path pool but refills missing bytes directly
from a per-turn OS-backed host source, so repeated random requests no longer
fail merely because the bootstrap seed was consumed. The per-request Web
Crypto 65,536-byte bound and explicit validation remain. Its contract is in
[tasks/native-engine-browser-300.md](tasks/native-engine-browser-300.md).

The completed native-engine-browser-301 Web Crypto digest slice adds
page and dedicated-worker `crypto.subtle.digest()` for SHA-1, SHA-256, SHA-384,
and SHA-512 using Rust digest implementations and the existing bounded
BufferSource/Promise surfaces. Its known-vector witness and local validation
are recorded in [tasks/native-engine-browser-301.md](tasks/native-engine-browser-301.md).

The completed native-engine-browser-302 Web Crypto HMAC-key slice adds
opaque page and dedicated-worker `CryptoKey` lifecycle plus bounded raw
HMAC `importKey()`, extractable `exportKey()`, `sign()`, and `verify()` for
SHA-1, SHA-256, SHA-384, and SHA-512. Its page/worker known-vector witness and
local evidence are recorded in [tasks/native-engine-browser-302.md](tasks/native-engine-browser-302.md).

The completed native-engine-browser-303 Web Crypto AES-GCM slice adds raw
AES-128/192/256 key lifecycle plus bounded page/worker encrypt/decrypt,
additional authenticated data, view-range preservation, and authentication
failure handling over the existing native host boundary. Its NIST vector,
round-trip, worker, and typed-error evidence are recorded in
[tasks/native-engine-browser-303.md](tasks/native-engine-browser-303.md).

The completed native-engine-browser-304 Web Crypto key-generation slice adds
native HMAC and AES-GCM secret-key generation using the existing OS-backed
realm pool. It validates default/explicit lengths and usages, preserves
opaque key identity and exportability, and proves generated keys through real
page/worker sign and encrypt/decrypt operations. Its focused evidence is
recorded in [tasks/native-engine-browser-304.md](tasks/native-engine-browser-304.md).

The completed native-engine-browser-305 Web Crypto derivation slice adds raw
HKDF/PBKDF2 base keys, RFC 5869/RFC 8018-style `deriveBits()`, and derived HMAC
or AES-GCM keys in page and dedicated-worker realms. The implementation keeps
base material non-extractable and realm-local, bounds input/output/iterations
and total input-byte work, and proves page/worker vectors plus real derived-key
operations. Its focused evidence is recorded in
[tasks/native-engine-browser-305.md](tasks/native-engine-browser-305.md).

The completed native-engine-browser-306 Web Crypto wrapping slice adds raw
HMAC/AES-GCM `wrapKey()` and `unwrapKey()` over the existing authenticated
AES-GCM owner in page and dedicated-worker realms. It preserves key bytes and
target hash/length/usages, rejects tampering and non-extractable sources, and
proves restored keys through real sign/verify and encrypt/decrypt operations.
Its focused evidence is recorded in
[tasks/native-engine-browser-306.md](tasks/native-engine-browser-306.md).

The completed native-engine-browser-307 Web Crypto JWK slice adds bounded
`oct` JWK import/export for HMAC and AES-GCM in page and dedicated-worker
realms. It preserves base64url key bytes and standard algorithm/usage metadata,
rejects contradictory `ext`/`key_ops` or algorithm fields, and proves the
restored keys with real crypto operations. Its focused evidence is recorded in
[tasks/native-engine-browser-307.md](tasks/native-engine-browser-307.md).

The completed native-engine-browser-308 Web Crypto block-mode slice adds
bounded AES-CBC and AES-CTR operations in page and dedicated-worker realms.
Raw import, generation, and derived-key targets support 128/192/256-bit keys;
CBC enforces a 16-byte IV and PKCS#7 padding, while CTR enforces a 16-byte
counter and 1..128-bit low-order counter length. Its NIST vectors, typed
errors, page/worker parity, and key-size coverage are recorded in
[tasks/native-engine-browser-308.md](tasks/native-engine-browser-308.md).

The completed native-engine-browser-309 Web Crypto asymmetric slice adds
Ed25519 key-pair generation, validated raw public-key import, OKP/Ed25519 JWK
import/export, and Rust-owned sign/verify operations to page and
dedicated-worker realms. Private/public usage partitioning, RFC 8032 signing,
point validation, key-pair consistency, tamper rejection, and realm-local
opaque key state are covered by the focused witness. RSA/EC algorithms,
PKCS#8/SPKI formats, key transfer, complete Web Crypto, and full Web IDL
parity remain issue #40 work; see
[tasks/native-engine-browser-309.md](tasks/native-engine-browser-309.md).

The completed native-engine-browser-310 Canvas 2D slice adds persistent,
bounded RGBA surfaces to page scripts and carries them through native layout,
display-list painting, screenshots, and document snapshots. It covers canvas
dimensions and reset behavior, fills, clears, paths, strokes, transforms,
gradients, compositing, image data, canvas-to-canvas drawing, PNG export, and
bounded text metrics/rendering. Its focused evidence and explicit limits are
recorded in
[tasks/native-engine-browser-310.md](tasks/native-engine-browser-310.md);
complete Canvas/Web IDL breadth and the remaining issue #40 production gates
remain open.

The completed native-engine-browser-311 image-source slice connects decoded
`<img>` and data-image pixels to Canvas 2D `drawImage()` across all supported
argument forms. It transfers and persists canvas origin-clean state, allowing
same-origin drawing while correctly blocking `SecurityError` readback after a
different-origin image is drawn. Its focused evidence is recorded in
[tasks/native-engine-browser-311.md](tasks/native-engine-browser-311.md);
full CORS image semantics, complete Canvas/Web IDL breadth, and the remaining
issue #40 production gates remain open.

The completed native-engine-browser-312 ImageBitmap slice adds bounded native
`ImageBitmap` wrappers and Promise-backed `createImageBitmap()` for the
supported image, canvas, ImageData, and ImageBitmap sources. Crop/resize
sampling, Canvas `drawImage()` integration, `instanceof` identity,
`close()` lifecycle errors, and origin-clean propagation are covered by the
focused Canvas witness. Video/media sources, complete CORS image semantics,
full Canvas/Web IDL breadth, and the remaining issue #40 production gates
remain open; exact evidence is recorded in
[tasks/native-engine-browser-312.md](tasks/native-engine-browser-312.md).

The completed native-engine-browser-313 OffscreenCanvas slice adds page-realm
constructable and transferred OffscreenCanvas surfaces on top of the retained
Canvas 2D owner. Placeholder transfer, standalone rendering, ImageBitmap
snapshots, Promise-backed PNG export, transfer-state errors, bounded dimension
reset, and cross-turn retained-pixel rehydration are covered by the focused
Canvas witness. Worker-realm installation, video/media sources, complete
Canvas/Web IDL breadth, and the remaining issue #40 production gates remain
open; exact evidence is recorded in
[tasks/native-engine-browser-313.md](tasks/native-engine-browser-313.md).

The completed native-engine-browser-314 Path2D and clipping slice adds
constructable reusable paths, bounded SVG-style path strings, transformed
`addPath()`, path overloads for Canvas fill/stroke/query operations, and
stateful `clip()` regions. Central software-raster clipping now applies to
fill, stroke, clear, text, and image writes and is preserved by
`save()`/`restore()`. Worker-realm installation, unsupported path grammar,
complete Canvas/Web IDL breadth, and the remaining issue #40 production gates
remain open; exact evidence is recorded in
[tasks/native-engine-browser-314.md](tasks/native-engine-browser-314.md).

The completed native-engine-browser-315 animated-image bridge carries bounded
GIF/APNG/WebP frame pixels, delays, loop metadata, and a synchronized
animation clock into the persistent page Canvas image-source adapter. Rust
compositor animation was already advancing frames; this slice keeps
`drawImage(img, ...)` in a retained page realm from freezing at one bootstrap
bitmap. Unsupported media/video sources, worker-realm Canvas installation,
complete Canvas/Image/Web IDL breadth, and the remaining issue #40 production
gates remain open; exact evidence is recorded in
[tasks/native-engine-browser-315.md](tasks/native-engine-browser-315.md).

The completed native-engine-browser-316 matrix/point slice adds bounded
2D `DOMMatrix`, `DOMMatrixReadOnly`, `DOMPoint`, and `DOMPointReadOnly`
identity and transform operations. Canvas `setTransform()`/`transform()` and
`Path2D.addPath()` accept those objects, while `getTransform()` and point
transforms return the corresponding Web IDL objects. Three-dimensional and
full matrix-string semantics remain out of scope; complete Canvas/Web IDL
breadth and the remaining issue #40 production gates remain open. Exact
evidence is recorded in
[tasks/native-engine-browser-316.md](tasks/native-engine-browser-316.md).

The completed native-engine-browser-317 slice makes the native runtime the
primary product path. Feature-enabled builds enable `native-engine` by
default, CLI parsing selects `native` by default, browser dispatch preserves
browser-free administration, and the standalone browser TUI now adapts its
navigation, observation, semantic actions, target selection, history, reload,
and PNG presentation through `BrowserRuntimeSession`. Explicit
`--browser-runtime chromium` retains the BrowserSession/CDP migration path;
complete Core Web Profile conformance, native workflow coverage, and
cross-platform certification remain issue #40 gates. Exact evidence is
recorded in [tasks/native-engine-browser-317.md](tasks/native-engine-browser-317.md).

The completed native-engine-browser-299 page-runtime-primitives slice adds
page `TextEncoder`/`TextDecoder`, bounded `atob`/`btoa`,
`structuredClone`, and public `EventTarget` support through the existing page
event and byte helpers, including object `handleEvent` listener identity.
Its focused witness and final local validation are recorded in
[tasks/native-engine-browser-299.md](tasks/native-engine-browser-299.md).

The completed EventSource/SSE slice is
[native-engine-browser-253](tasks/native-engine-browser-253.md). It adds a
bounded persistent HTTP(S) event-stream owner with shared URL/security policy,
same-origin cookie and response-cookie handoff, LF/CRLF/CR parsing, named and
multiline messages, reconnect state, `Last-Event-ID`, and serialized page
event/mutation delivery. The scoped local content suite is green; the broader
native parity and production-promotion gates remain open on issue #40.

The completed incremental Fetch response-body slice is
[native-engine-browser-254](tasks/native-engine-browser-254.md). It transfers
authorized response bodies from the process-backed loader as bounded
transport chunks through a persistent `ReadableStream`, waits correctly for
pending reads and terminal delivery, and preserves cross-turn Blob/File and
ReadableStream constructor identity. Convenience body methods remain bounded
full-body reads; demand-driven transport, body disturbance, and complete Fetch
Streams/Web IDL parity remain open, and the broader native parity and
production-promotion gates remain open on issue #40.

The completed semantic action scroll-into-view slice is
[native-engine-browser-255](tasks/native-engine-browser-255.md). Native
semantic element actions now perform a bounded nearest root scroll when a
resolved target is outside the active viewport, including selected child
frames, while explicit `point=` clicks remain coordinate-stable and native
preflight remains side-effect-free. The focused FormData and point-dispatch
regressions plus the ordered 533-test native integration suite are green
locally; the broader native parity and production-promotion gates remain open
on issue #40.

The completed Fetch demand/cancellation slice is
[native-engine-browser-256](tasks/native-engine-browser-256.md). Native
response streams now request one bounded transport part at a time, retain
queued parts only behind explicit reader/body demand, and propagate reader or
stream cancellation through the content-process owner to the live HTTP body.
The focused content-process suite and cancellation witness are green locally;
body disturbance, tee/BYOB/piping, trailers, and complete Fetch Streams/Web IDL
parity remain open on issue #40.

The completed Fetch body-ownership slice is
[native-engine-browser-257](tasks/native-engine-browser-257.md). Native
responses now expose bounded `bodyUsed` state, reject repeated body
consumption, reject clones after body disturbance or locking, and preserve
independent reads through clones created before consumption. The focused
Fetch-named integration suite and body-ownership witness are green locally;
shared tee/BYOB/piping, trailers, and complete Fetch Streams/Web IDL parity
remain open on issue #40.

The completed Fetch clone-queue slice is
[native-engine-browser-258](tasks/native-engine-browser-258.md). Cloned
response bodies now share bounded per-branch queues, transport demand pauses
when an unread branch reaches its queue bound, and the live response remains
owned until all clone readers cancel or finish. The focused Fetch suite and
two-reader cancellation witness are green locally; full tee algorithms,
BYOB/piping, trailers, and complete Fetch Streams/Web IDL parity remain open
on issue #40.

The completed Fetch Request body-ownership slice is
[native-engine-browser-259](tasks/native-engine-browser-259.md). Native
`Request` objects now expose bounded `bodyUsed` state, reject reuse and
cloning after a source body is consumed, permit an explicit replacement body
through fetch options, and preserve independent pre-consumption clones. The
focused Request ownership witness is green locally; full Request body
streams, body convenience methods, and complete Fetch Streams/Web IDL parity
remain open on issue #40.

The completed Fetch Request body-surface slice is
[native-engine-browser-260](tasks/native-engine-browser-260.md). Native
Requests now expose bounded static `ReadableStream` bodies and one-shot
`text()`, `json()`, `blob()`, `arrayBuffer()`, and `bytes()` consumers; stream
disturbance and locking participate in `bodyUsed` and clone ownership, and
Fetch dispatches the captured body bytes. Caller-supplied streaming uploads,
`Request.formData()`, and complete Fetch Streams/Web IDL parity remain open on
issue #40.

The completed Fetch Request FormData slice is
[native-engine-browser-261](tasks/native-engine-browser-261.md). Native
`Request.formData()` now parses bounded URL-encoded fields and multipart
fields/files with byte-preserving File payloads, while unsupported body media
types reject explicitly. Streaming upload sources and complete Fetch
Streams/Web IDL parity remain open on issue #40.

The completed native ReadableStream-source slice is
[native-engine-browser-262](tasks/native-engine-browser-262.md). Native
`ReadableStream` now accepts bounded underlying sources with `start`,
`pull`, `cancel`, controller enqueue/close/error, demand-driven reads, and
source-backed cancellation. BYOB readers, piping, transfer strategies, and
complete Streams/Web IDL parity remain open on issue #40.

The completed native ReadableStream reader-lifecycle slice is
[native-engine-browser-263](tasks/native-engine-browser-263.md). Native
stream terminal state now settles every outstanding reader `closed` promise,
including readers released before close, and rejects it consistently when a
source errors; reader cancellation shares the same terminal notification path.
BYOB readers, piping, transfer strategies, and complete Streams/Web IDL parity
remain open on issue #40.

The completed native ReadableStream tee slice is
[native-engine-browser-264](tasks/native-engine-browser-264.md). Native
`ReadableStream.prototype.tee()` now shares one upstream reader across two
bounded branches, propagates values and terminal errors/close, applies the
existing queue bound as backpressure, and cancels the upstream only after both
branches cancel. Page-created stream tee algorithms, BYOB readers, piping,
transfer strategies, and complete Streams/Web IDL parity remain open on issue
#40.

The completed native stream-backed Request body slice is
[native-engine-browser-265](tasks/native-engine-browser-265.md). Native
`Request` accepts usable page-created streams, convenience body methods drain
bounded byte chunks, `Request.clone()` tees an unconsumed stream body, and
Fetch accepts both Request-owned and direct stream upload bodies through the
existing Rust request transport. Full upload streaming/progress semantics,
BYOB readers, piping, transfer strategies, and complete Fetch Streams/Web IDL
parity remain open on issue #40.

The completed native stream-backed Response slice is
[native-engine-browser-266](tasks/native-engine-browser-266.md). Native
`Response` accepts usable page-created streams, exposes them through the
existing body/`bodyUsed` surface, clones them through bounded tee branches, and
drains them for text/json/blob/arrayBuffer/bytes consumers. Fetch-created
responses retain their transport stream implementation; upload progress,
BYOB readers, piping, transfer strategies, and complete Fetch Streams/Web IDL
parity remain open on issue #40.

The completed native writable-stream and piping slice is
[native-engine-browser-267](tasks/native-engine-browser-267.md). Native
`WritableStream` now provides bounded serialized sink writes, writer lifecycle
and lock state, close/abort hooks, and `ReadableStream.pipeTo()`/
`pipeThrough()` with source cancellation and sink-abort propagation. Full
WritableStream/Web IDL semantics, TransformStream, BYOB readers, transfer
strategies, upload progress, and complete Fetch Streams parity remain open on
issue #40.

The completed native TransformStream slice is
[native-engine-browser-268](tasks/native-engine-browser-268.md). Native
`TransformStream` now creates a connected writable/readable pair with bounded
`start`/`transform`/`flush` processing and controller enqueue/error/terminate
operations, making `pipeThrough(new TransformStream(...))` functional. Full
controller strategy semantics, TransformStream/Web IDL parity, BYOB readers,
transfer strategies, upload progress, and complete Fetch Streams parity remain
open on issue #40.

The completed native byte-stream slice is
[native-engine-browser-269](tasks/native-engine-browser-269.md). Byte-backed
`ReadableStream` owners now validate byte sources, honor bounded
`highWaterMark`/`size` strategies for `desiredSize`, expose BYOB readers and
`byobRequest` response methods, preserve partial buffers, and retain byte mode
through Fetch delivery and tee branches. Full Web IDL descriptors, transfer
strategies, upload progress, and complete Fetch Streams parity remain open on
issue #40.

The completed native resource/lifecycle resilience slice is
[native-engine-browser-270](tasks/native-engine-browser-270.md). Failed
optional HTTP(S) stylesheet and external-script loads now leave the main
document committed and dispatch a bounded element `error` event; successful
resource loading remains a `load` event. Normal replacement and child-frame
`pagehide`/`pageshow` events now expose `persisted: false`, matching the
non-BFCache navigation path. Static module dependency failure policy,
resource timing/concurrency, BFCache restoration, and complete lifecycle/Web
IDL parity remain open on issue #40.

The completed native module-failure isolation slice is
[native-engine-browser-271](tasks/native-engine-browser-271.md). A loaded
external module whose static dependency fails now removes that incomplete
module graph from evaluation and dispatches one root-script `error` event,
while the owning document still commits and reaches its normal lifecycle.
Static graph limits and inline-module error-event identity remain separately
bounded; resource scheduling, BFCache restoration, and complete module/Web
IDL parity remain open on issue #40.

The completed native external-script failure slice is
[native-engine-browser-272](tasks/native-engine-browser-272.md). External
classic and module scripts that throw during evaluation now dispatch one
owning-element `error` event, suppress their provisional `load`, and leave the
document lifecycle able to complete. The module graph is never published as a
successful script after evaluation failure; full window error reporting,
inline-script identity, and complete script/Web IDL semantics remain open on
issue #40.

The completed native inline-script failure slice is
[native-engine-browser-273](tasks/native-engine-browser-273.md). Inline classic
and module roots now retain their owning script element identity through local
and content-process staging. An ignorable inline evaluation failure dispatches
one owning-element `error` event, prevents the failed script from continuing,
and still allows the committed document to reach `readyState === "complete"`.
Full window error reporting, parser-accurate execution timing, and complete
script/Web IDL semantics remain open on issue #40.

The completed native script-error reporting slice is
[native-engine-browser-274](tasks/native-engine-browser-274.md). Ignorable
classic and module evaluation failures now report a bounded `ErrorEvent` to
the page window in addition to the owning script-element `error`; the event
exposes message, filename, line/column, and the underlying `Error`, while
`window.onerror` receives its five-argument callback form. Module diagnostics
retain the underlying exception message, and infrastructure failures remain
hard errors. Parser-accurate timing and complete script/Web IDL semantics
remain open on issue #40.

The completed native unhandled-rejection slice is
[native-engine-browser-275](tasks/native-engine-browser-275.md). The persistent
QuickJS realm now tracks bounded unhandled Promise rejections after its
microtask checkpoint, suppresses rejections that acquire a handler, preserves
event order, and dispatches cancelable `PromiseRejectionEvent` instances to
the page window, including `window.onunhandledrejection`. Rejection reasons
are transported as bounded text and the `promise` field is currently `null`;
`rejectionhandled`, structured reason identity, parser-accurate timing, and
complete Promise/Web IDL semantics remain open on issue #40.

The completed native rejection-settlement slice is
[native-engine-browser-276](tasks/native-engine-browser-276.md). Rejections
that were already reported and later acquire a handler now dispatch a
non-cancelable `rejectionhandled` event through both the window listener and
property-handler surfaces; pre-checkpoint handlers remain suppressed. The
native queue preserves promise identity internally, bounds reported state,
and keeps event order. The event still exposes a bounded textual reason with
`promise === null`; structured identity, parser-accurate timing, and complete
Promise/Web IDL semantics remain open on issue #40.

The completed bounded script-scheduling slice is
[native-engine-browser-277](tasks/native-engine-browser-277.md). Parser-blocking
and async scripts now retain discovery order relative to one another, while
deferred scripts and modules remain ordered after that work; the content
process witness now observes `blocking-1 → async → blocking-2 → defer`. True
network completion-order scheduling, parser-stream execution, and complete
script/lifecycle/Web IDL semantics remain open on issue #40.

The completed bounded idle-callback slice is
[native-engine-browser-278](tasks/native-engine-browser-278.md).
`requestIdleCallback` and `cancelIdleCallback` now retain bounded callback
state, honor zero-or-future timeout deadlines, expose `didTimeout` and a
bounded `timeRemaining()` budget, and run after due timers and animation frames
on the shared local/content-process realm. Full background scheduling,
task-source fairness, and browser idle-budget arbitration remain open on issue
#40.

The completed dynamically attached script slice is
[native-engine-browser-279](tasks/native-engine-browser-279.md). Newly
connected classic inline scripts now execute synchronously in the page realm,
including elements created before a later attachment, and a bounded
single-shot ledger prevents reruns after moves or text changes. Direct dynamic
external/module sources enter the existing process-backed loader path. Nested
external/module discovery from a dynamically executing script still needs
event-loop loader handoff, as do dynamic network effects; full script,
parser-streaming, and Web IDL semantics remain open on issue #40.

The completed nested dynamic-script loader slice is
[native-engine-browser-280](tasks/native-engine-browser-280.md). External and
module sources discovered by an executing dynamic script now return to the
content owner, pass through the existing resource/module policy, and execute
recursively under a bounded loader-turn limit. The witness covers a dynamic
external module that attaches a second external classic script. Dynamic
Fetch/WebSocket/EventSource handoff, parser streaming, network completion
timing, and complete script/lifecycle/Web IDL semantics remain open on issue
#40.

The completed dynamic network-effects slice is
[native-engine-browser-281](tasks/native-engine-browser-281.md). Fetch,
WebSocket, and EventSource commands emitted by dynamically loaded scripts now
enter the existing bounded content event loop, while background event pumping
remains separate from merely having an open connection. Dynamic Fetch Promise
continuations publish their DOM effects, and the existing page Fetch,
WebSocket, and EventSource witnesses remain green. Parser streaming, true
network completion ordering, complete stream/body Web IDL semantics, and full
native/CDP parity remain open on issue #40.

The completed profile/contract task is
[native-engine-browser-000](tasks/native-engine-browser-000.md). It freezes
external HTTP(S) navigation, standards/web-platform ownership, Glass API
parity, security boundaries, supported platforms, conformance thresholds,
performance budgets, and explicit exclusions. It is a scope gate, not a claim
that the current native backend already implements those capabilities.

The first executable browser-complete batch is
[native-engine-browser-001](tasks/native-engine-browser-001.md). It adds the
typed runtime substrate—runtime lifecycle, cancellation, task/microtask
ordering, bounded privacy-safe traces, startup rollback, and terminal close—
while keeping the current deterministic local engine boundary. It does not yet
claim network, JavaScript, process isolation, or browser parity.

The next executable network batch is
[native-engine-browser-002](tasks/native-engine-browser-002.md). It adds
bounded external HTTP(S) HTML navigation through the native backend, including
redirect limits, response-size and HTML MIME checks, UTF-8 decoding, normalized
HTTP(S) origins, and native-only integration coverage. It does not yet claim
subresources, JavaScript, cookies/cache, CORS/CSP, charset sniffing, process
isolation, or browser parity.

The next runtime batch is
[native-engine-browser-003](tasks/native-engine-browser-003.md). It adds a
bounded typed Tokio worker over the single runtime state, routes asynchronous
native initialization/navigation commits through that worker, and reports
worker cancellation/crash failures explicitly. It is still in-process and does
not yet claim content-process isolation, OS sandboxing, supervisor restart, or
browser parity.

The completed process-control batch is
[native-engine-browser-004](tasks/native-engine-browser-004.md). It adds the
`glass-native-content-worker` helper inside the existing `glass-browser` crate,
bounded request-ID-correlated framed IPC, explicit ping/start/commit/close
acknowledgements, and a fail-closed requirement that external HTTP(S)
initialization/navigation have a live helper. The parent still owns bounded
resource loading and document construction; resource transfer, content
execution, OS sandboxing, supervisor recovery, and browser parity remain open.

The completed resource-transfer batch is
[native-engine-browser-005](tasks/native-engine-browser-005.md). The child now
invokes the shared bounded HTTP(S) loader, parses the HTML tree, and returns
only validated final-URL metadata plus a size-capped typed DOM snapshot. Load
deadlines, frame/document quotas, malformed-transfer detection, and child
poisoning are explicit; local resources remain in-process. The parent
reconstructs the DOM and reparses stylesheet sources, so computed-style
isolation, content execution, sandboxing, supervisor recovery, and browser
parity remain open.

The completed computed-style batch is
[native-engine-browser-006](tasks/native-engine-browser-006.md). External
documents now carry one typed computed-style record per child-parsed node; the
parent validates the bounded snapshot and uses it for layout/visibility without
reparsing stylesheet sources. Local resources retain the direct stylesheet
path. CSS diagnostics transfer, script execution, sandboxing, supervisor
recovery, and browser parity remain open.

The completed mutation-ownership batch is
[native-engine-browser-007](tasks/native-engine-browser-007.md). The child now
retains each external document, applies bounded click/type mutations
transactionally, and returns a fresh snapshot plus typed privacy-safe effects;
the parent validates and publishes the revision exactly once. Mutation timeout,
malformed-transfer, and child-rejection paths poison the worker without false
success or CDP fallback. Scroll and link navigation remain explicit
parent-owned handoffs; standards events, script execution, diagnostics
transfer, sandboxing, supervisor recovery, and browser parity remain open.

The completed recovery batch is
[native-engine-browser-008](tasks/native-engine-browser-008.md). Content-worker
spawn, exit, transport, timeout, protocol, rejection, and invalid-transfer
failures now have a typed class. A failed action is never replayed or silently
fallen back; external navigation is the explicit fresh-worker recovery
boundary, and shutdown tolerates an already-exited child. OS-specific
sandboxing, cross-platform crash/restart coverage, standards events, script
execution, network security, and browser parity remain open.

The completed sandbox-launch batch is
[native-engine-browser-009](tasks/native-engine-browser-009.md). Linux now
requires Bubblewrap with isolated user/PID/UTS/IPC namespaces, read-only
runtime mounts, private `/tmp`, parent-death cleanup, and `no_new_privs`;
macOS uses a deny-by-default Seatbelt profile; Windows uses a retained Job
Object with process-count and kill-on-close limits. Missing policy support is a
typed startup failure, never an implicit unsandboxed fallback. Full origin/site
isolation, network mediation, restricted Windows tokens, cross-platform
containment evidence, and the remaining browser gates remain open.

The completed redirect/charset batch is
[native-engine-browser-010](tasks/native-engine-browser-010.md). The shared
loader now rejects credential-bearing or non-HTTP(S) redirects before follow,
keeps the eight-hop limit and final-origin validation, and decodes bounded
UTF-8, UTF-16, Latin-1, and Windows-1252 HTML responses. The child and parent
share this policy; cookies/cache, CORS/CSP, mixed content, service workers,
permissions, subresources, full WHATWG encoding sniffing, script execution,
and browser parity remain open.

The completed stateful network batch is
[native-engine-browser-011](tasks/native-engine-browser-011.md). The
process-backed loader now retains bounded session-only cookies and a bounded
in-memory document cache across same-child navigations, applies domain/path/
secure cookie matching, and denies cache reuse for explicit no-cache, private
variant, or Set-Cookie responses. Cookie/cache state is never persisted or
logged. Bounded document-navigation freshness/revalidation is covered by
slice 346; full subresource/Fetch HTTP freshness, CORS/CSP, mixed content,
service-worker routing, permissions, subresources, complete encoding
sniffing, origin/referrer request policy, script execution, and browser parity
remain open.

The BE-02c scope was explicit origin/referrer request policy and cross-origin
request mediation, followed by CSP/mixed-content, service-worker, permission,
and subresource work.

The completed origin/referrer batch is
[native-engine-browser-012](tasks/native-engine-browser-012.md). Native
top-level navigation now derives a strict-origin-when-cross-origin referrer
from the previously committed URL, re-evaluates it at every manually
validated redirect hop, and keeps redirect cookies transactional until the
final document succeeds. Same-origin full URLs, cross-origin origin-only
referrers, HTTPS downgrade suppression, and child-wire policy validation are
covered. CORS/CSP, mixed content, service workers, permissions, subresources,
full HTTP cache semantics, script execution, and browser parity remain open.

The BE-02d scope was CORS/CSP, mixed-content, and initial subresource
mediation for the native document path.

The completed stylesheet-subresource batch is
[native-engine-browser-013](tasks/native-engine-browser-013.md). The child
now discovers a bounded number of link stylesheets, applies CSP
style-src/default-src and HTTPS mixed-content checks before request, fetches
validated text/css resources with the shared cookie/redirect/referrer limits,
and includes accepted rules in the child-owned computed-style snapshot.
Images, media, fonts, scripts, fetch/XHR, service workers, permissions,
complete CSP/CORS, and browser parity remain open.

The completed BE-02e policy-foundation batch is
[native-engine-browser-014](tasks/native-engine-browser-014.md). The shared
loader now has typed resource-family CSP source lists, credential-free
HTTP(S) subresource resolution, a common HTTPS mixed-content check, and
credential-aware CORS origin/response authorization. Stylesheet loading uses
the shared URL/CSP/mixed-content path. Script/module execution, fetch/XHR
callers and preflights, image/media/font/frame/worker loading, service
workers, permissions, complete CSP, and browser parity remain open; the
helpers alone do not claim those capabilities.

The remaining BE-02e implementation gate is to wire the policy into real
script/module and connect/fetch request callers, then add the remaining
resource classes without leaking response data or bypassing process ownership.

The completed bounded child-fetch batch is
[native-engine-browser-015](tasks/native-engine-browser-015.md). A running
native engine can now issue one child-owned, GET-only fetch from its current
external document. The child applies `connect-src`/`default-src`, URL and
HTTPS mixed-content checks, bounded redirects, credentials policy, CORS
`Origin`/ACAO authorization, response-size limits, and typed IPC transfer;
the focused process-backed filter passed 13/13. This is a kernel primitive,
not `window.fetch`: custom methods/headers/bodies, preflights, streams,
service workers, JavaScript/Web IDL, remaining resource classes, and browser
parity remain open.

The completed JavaScript-realm batch is
[native-engine-browser-016](tasks/native-engine-browser-016.md). The native
backend now exposes bounded ECMAScript evaluation through an optional,
feature-gated QuickJS realm. Local documents use an owner-side persistent
realm; external documents use the sandboxed content worker, and full
navigation resets page globals. Results are bounded JSON with explicit
source, result, memory, stack, and execution limits. Timers, modules, script
loading, Fetch/XHR integration, service workers, remaining resource classes,
and browser parity remain open.

The completed JavaScript host-view batch is
[native-engine-browser-017](tasks/native-engine-browser-017.md). Each
evaluation now refreshes a bounded read-only `window`/`document` projection
with location/origin, viewport, title/text, form state, and explicit element
finders in both local and child-owned realms. Synchronous results retain their
direct JSON value, and top-level `await` completes bounded QuickJS jobs before
the same result conversion. Live Web IDL identity, DOM mutation, event
dispatch, timers, modules, page-script loading, Fetch/XHR, remaining resource
classes, and browser parity remain open.

The next BE-02/BE-04 gate is transactional JavaScript-driven DOM mutation and
event integration, followed by the broader Fetch request/response model
through the existing child policy boundary.

The completed JavaScript DOM-mutation batch is
[native-engine-browser-018](tasks/native-engine-browser-018.md). JavaScript
can now emit bounded `click()`, form-state, and attribute commands. Glass
validates and applies each script batch to a cloned native document, commits
one revision, and refreshes the host view; the child process performs the same
ownership and transfer sequence for external pages. Live object identity,
listener dispatch, navigation from script, timers, modules, Fetch/XHR,
page-script loading, remaining resource classes, and browser parity remain
open.

The completed JavaScript event/focus batch is
[native-engine-browser-019](tasks/native-engine-browser-019.md). The persistent
realm now owns bounded target-local listener registration/removal, `Event` and
`CustomEvent` dispatch with cancellation, and script-visible `focus()`/
`blur()` transitions. Focus and blur cross the same typed command boundary and
are committed with one revision in both local and child-owned documents;
scripted `click()` activation is canceled when its target listener calls
`preventDefault()`. Ancestor propagation/capture, Rust-action listener
dispatch, default-action ordering, mutation invalidation, timers, modules,
Fetch/XHR, page-script loading, remaining resource classes, and browser parity
remain open.

The completed JavaScript event-graph batch is
[native-engine-browser-020](tasks/native-engine-browser-020.md). Projected
elements now expose bounded parent links, and dispatch runs snapshot-based
capture, target, and bubble phases with `stopPropagation()`,
`stopImmediatePropagation()`, and `once` handling. Rust semantic actions still
do not re-enter the page realm.

The completed Rust-action event bridge is
[native-engine-browser-021](tasks/native-engine-browser-021.md). Committed
semantic action effects now re-enter the existing local or sandboxed child
realm as typed host-event metadata; callback mutations return through the same
clone-and-transfer owner path. The first bridge is deliberately post-action:
callback mutations receive an additional revision, and `preventDefault()` does
not yet roll back or suppress an already-committed Rust default action. The
transactional click preflight is recorded separately below.

The completed cancelable-click batch is
[native-engine-browser-022](tasks/native-engine-browser-022.md). Local and
child-owned semantic clicks now preflight focus and click listeners on a clone,
apply callback commands, honor `preventDefault()`, and commit the final state
and effects exactly once. Pages without a JavaScript realm retain the Rust-only
path.

The completed transactional type-event batch is
[native-engine-browser-023](tasks/native-engine-browser-023.md). Local and
child-owned type actions now apply the value on a clone, dispatch focus,
input, and change in order, apply callback commands, and commit one revision.
The current host command sink also keeps captured callback setters connected to
the current bounded evaluation buffer without claiming full live Web IDL
identity.

The completed script-navigation batch is
[native-engine-browser-024](tasks/native-engine-browser-024.md). Top-level
script link clicks now hand off one validated navigation request to the local
history/resource owner or to the parent after child-owned validation and
transfer. Full navigations reset the realm, same-document navigation retains
it, and no path silently falls back to CDP. Click and navigation currently use
separate revisions; target contexts, form submission, timers, modules,
page-script loading, and Fetch/XHR remain open.

The completed relative-URL/history batch is
[native-engine-browser-025](tasks/native-engine-browser-025.md). Relative and
root-relative HTTP(S) links now resolve against the current document, while
local and external fragment links stay same-document and retain their page
realm without a redundant fetch. Target contexts, form submission, unload
ordering, timers, modules, page-script loading, and Fetch/XHR remain open.

The completed bounded inline-page-script batch is
[native-engine-browser-026](tasks/native-engine-browser-026.md). Local
prepared navigations and HTTP(S) content-process loads now execute up to 32
bounded inline JavaScript sources in the owning persistent realm. Typed DOM
commands apply against the parsed document before publication, load-time link
activation is rejected, and failed local scripts do not publish a partial
navigation. External `src` scripts, modules, parser timing, timers, Fetch/XHR,
and full Web IDL identity remain open.

The completed classic external-script batch is
[native-engine-browser-027](tasks/native-engine-browser-027.md). HTTP(S)
content processes now resolve accepted classic `src` scripts in document order,
apply the existing script CSP/default-src, mixed-content, redirect,
referrer/cookie, MIME, and byte policies, and execute them in the persistent
child realm alongside inline sources. Module/unknown types are not fetched;
local fixture/data subresources, parser timing, timers, Fetch/XHR, and full Web
IDL identity remain open.

The completed bounded GET-form batch is
[native-engine-browser-028](tasks/native-engine-browser-028.md). Local and
child-owned forms now encode named enabled controls into a bounded query,
support `form.submit()`/`requestSubmit()`, and route submit-button script clicks
through the same navigation owner. POST/multipart, full constraint validation,
complete submission lifecycle/event parity, target contexts, module timing,
timers, Fetch/XHR, and the remaining resource classes remain open.

The completed bounded module-root batch is
[native-engine-browser-029](tasks/native-engine-browser-029.md). Local and
HTTP(S) documents now classify and execute bounded inline/external module roots
through QuickJS's module evaluator in document order, retaining the owning
realm and typed command boundary. Static import graphs, dynamic `import()`,
parser timing, POST/submission lifecycle/default-action ordering, target
contexts, timers, Fetch/XHR, and the remaining resource classes remain open.

The completed bounded static-module-graph batch is
[native-engine-browser-030](tasks/native-engine-browser-030.md). HTTP(S)
content processes now prefetch bounded relative/absolute static module
dependencies under the owning document's script policy and expose them through
QuickJS's in-memory loader, including duplicate/cycle bounds. Bare specifiers,
dynamic `import()`, import maps, parser timing, POST/submission
lifecycle/default-action ordering, target contexts, timers, Fetch/XHR, and the
remaining resource classes remain open.

The completed bounded literal-dynamic-import batch is
[native-engine-browser-031](tasks/native-engine-browser-031.md). Literal
`import("...")` calls now reuse the policy-checked module graph and a bounded
QuickJS job drain, preserving module namespace resolution and promise callback
effects. Computed specifiers, bare packages/import maps, parser timing,
POST/submission lifecycle/default-action ordering, target contexts, timers,
Fetch/XHR, and the remaining resource classes remain open.

The completed bounded task-turn batch is
[native-engine-browser-032](tasks/native-engine-browser-032.md). Native page
realms now drain `queueMicrotask` jobs per evaluation/event turn and retain
bounded `setTimeout` callbacks for the next deterministic host turn in both
local and child-owned realms. Wall-clock delays, `setInterval`, animation/idle
callbacks, parser timing, POST/submission lifecycle/default-action ordering,
target contexts, Fetch/XHR, and the remaining resource classes remain open.

The completed bounded keyboard-input batch is
[native-engine-browser-033](tasks/native-engine-browser-033.md). Native
semantic `KeyPress` actions now route through local and child-owned focused
text controls, dispatch cancelable `keydown`/`input`/`keyup` callbacks with
bounded key metadata, and refresh persistent host wrappers before callbacks.
Printable keys append, `Backspace` removes the final scalar, and `Delete` is a
bounded end-of-value no-op. Selection, IME, navigation keys, `beforeinput`,
form defaults, and modifier shortcuts remain open.

The completed bounded GET-form lifecycle batch is
[native-engine-browser-034](tasks/native-engine-browser-034.md). Cancellable
`submit` events now run before GET query serialization for `requestSubmit()`
and submit-button defaults, callback mutations are retained, direct
`form.submit()` remains event-free, and local/child semantic submit buttons
hand off navigation through their existing owners.

The completed bounded urlencoded-POST batch is
[native-engine-browser-035](tasks/native-engine-browser-035.md). Forms with
`method="post"` now serialize the same bounded enabled named controls into an
`application/x-www-form-urlencoded` request body and send it through the
existing parent/content-process loader, preserving submit cancellation and
redirect/referrer/cookie policy. Unsupported methods and multipart/text/plain
encodings fail explicitly. Full constraint validation, submitter serialization,
target contexts, multipart bodies, and unload ordering remain open.

The completed bounded parser-time script-ordering batch is
[native-engine-browser-036](tasks/native-engine-browser-036.md). Classic
parser-blocking scripts, external async scripts, deferred classics, and
default-deferred module roots now use one deterministic local/child ordering
contract. Incremental parsing, completion-order races, script event timing,
and dynamic insertion remain open.

The completed bounded page-lifecycle batch is
[native-engine-browser-037](tasks/native-engine-browser-037.md). Local and
child-owned realms now deliver `DOMContentLoaded` to the document and then
`load` to the window after the accepted script schedule, retaining callback
mutations through the existing typed owner path. Ready-state transitions,
resource-specific events, unload/pagehide, completion races, and full
task-source timing remain open.

The completed bounded form-validation batch is
[native-engine-browser-038](tasks/native-engine-browser-038.md). Local and
child-owned owners now dispatch bounded non-bubbling `invalid` events for
required controls before blocking interactive submission, and valid submit
callbacks receive `event.submitter` for button activation and
`requestSubmit(button)`. Direct `form.submit()` remains validation-free.
Full constraint-validation APIs, submitter serialization, multipart encoding,
and target contexts remain open.

The completed bounded ready-state lifecycle batch is
[native-engine-browser-039](tasks/native-engine-browser-039.md). Local and
child-owned realms now expose `loading`, `interactive`, and `complete` at the
corresponding parser/lifecycle boundaries, dispatch `readystatechange` at the
interactive and complete transitions, and deliver `DOMContentLoaded` before
window `load`. Pages without scripts still expose a persistent realm with
final `document.readyState === "complete"`. Resource-specific completion,
unload/pagehide/pageshow, wall-clock races, and full task-source timing remain
open.

The completed bounded submitter-serialization batch is
[native-engine-browser-040](tasks/native-engine-browser-040.md). Successful
submit buttons now contribute bounded `name`/`value` pairs to local and
child-owned GET and urlencoded-POST requests, with the typed submitter checked
again by the navigation owner. Form `novalidate` and submitter
`formnovalidate` bypass the bounded required-control check while preserving
submit events and serialization. Image coordinates, target contexts,
multipart/text/plain, full constraint validation, and
FormData/Web IDL parity remain open.

The next BE-02/BE-03/BE-04 gate is resource-specific completion and full task
ordering, followed by multipart, full constraint-validation, external form
ownership, and the remaining browser-context primitives.

The completed bounded resource-lifecycle slice is
[native-engine-browser-041](tasks/native-engine-browser-041.md). Successful
external stylesheet/script completion events now reach their owning elements at
the typed owner boundary before `DOMContentLoaded`, with deterministic
document-order delivery and callback mutation commit. Failed resource error
events, dynamic insertion, resource timing, and full task-source concurrency
remain separate gates.

The completed bounded replacement-navigation lifecycle slice is
[native-engine-browser-042](tasks/native-engine-browser-042.md). Full
replacement navigations now deliver window `pagehide` then `unload` before
resource replacement and `pageshow` after the new page is published; local and
child owners expose the same order through bounded effects and typed callback
mutation. Cancelable `beforeunload`, bfcache/history-traversal parity, and full
HTML navigation task ordering remain open.

The completed bounded same-document navigation slice is
[native-engine-browser-043](tasks/native-engine-browser-043.md). GET fragment
changes retain the current document/realm, avoid a reload, update the URL owner,
and dispatch window `hashchange` with `oldURL`/`newURL` in both local and child
paths. `beforeunload`, `popstate`, bfcache/history lifecycle parity, and full
HTML navigation task ordering remain open.

The completed bounded external form-ownership slice is
[native-engine-browser-044](tasks/native-engine-browser-044.md). Controls with
an explicit `form="id"` now associate with the matching form even when they
are outside it; explicit ownership overrides ancestry, unresolved references
do not fall back, and local/child validation and GET/urlencoded-POST
serialization preserve document order. External submit buttons are accepted
by `requestSubmit(button)` through the typed owner path. Multipart/text/plain,
full constraint validation, target contexts, and the remaining browser-context
primitives remain open.

The completed bounded POST-encoding slice is
[native-engine-browser-045](tasks/native-engine-browser-045.md). POST forms now
carry explicit bounded `multipart/form-data` and `text/plain` bodies through
the parent/content-process request boundary, including the multipart boundary
header and redirect method/body reset. File parts, FormData/Web IDL identity,
full constraint validation, target contexts, and the remaining browser-context
primitives remain open.

The completed bounded navigation-cancellation/history-event slice is
[native-engine-browser-046](tasks/native-engine-browser-046.md). Replacement
navigations now dispatch cancelable window `beforeunload` before
`pagehide`/`unload`, honor `preventDefault()` and non-empty `returnValue`, and
avoid resource loading when canceled. Same-document history traversal now
dispatches window `popstate` before `hashchange` in local and child owners.
Prompts, bfcache/session-history parity, cross-document traversal restoration,
and full task-source semantics remain open.

The completed bounded due-time timer-turn slice is
[native-engine-browser-047](tasks/native-engine-browser-047.md). Local and
child realms now retain normalized `setTimeout` due times, drain only timers
that are due on a later host turn, preserve due-time/ID ordering, and honor
`clearTimeout`. There is still no background page event loop; intervals,
animation/idle callbacks, task-source fairness, and full wall-clock scheduling
remain open.

The completed bounded submitter-override slice is
[native-engine-browser-048](tasks/native-engine-browser-048.md). Local and
child-owned submissions now apply validated `formaction`, `formmethod`, and
`formenctype` overrides before request construction, including the effective
POST content type across the content-process boundary. Form target contexts,
dialog submission, file parts, and general form-control/Web IDL identity
remain open.

The completed bounded repeating-timer slice is
[native-engine-browser-049](tasks/native-engine-browser-049.md). Local and
child-owned realms now expose `setInterval`/`clearInterval`; each due callback
runs at most once on a supplied host turn, reschedules from that turn's
monotonic time, and can cancel itself. There is no background page loop or
task-source fairness; animation and idle callbacks remain open.

The completed bounded common-constraint slice is
[native-engine-browser-050](tasks/native-engine-browser-050.md). Local and
child-owned forms now validate required, email/URL, UTF-16 length, and numeric
min/max/step constraints before the existing ordered `invalid` events and
submit handoff. Pattern/file constraints and full `ValidityState` Web IDL
identity remain open; the bounded validation API is covered by 059.

The completed bounded script-fetch slice is
[native-engine-browser-051](tasks/native-engine-browser-051.md). Explicit
evaluations in process-backed HTTP(S) documents can now issue policy-owned GET
`fetch()` requests and resolve bounded response text/JSON promises, including
typed DOM callback mutations. Page-load fetch scheduling, non-GET uploads,
XHR/WebSocket, and full Fetch Web IDL identity remain open.

The completed bounded page-load fetch slice is
[native-engine-browser-052](tasks/native-engine-browser-052.md). Fetches from
initial page scripts and lifecycle evaluation now settle before the child
publishes its first document snapshot, including typed callback DOM mutations.
Callback navigation during initial publication, non-GET uploads, XHR/WebSocket,
and full Fetch Web IDL identity remain open.

The completed bounded same-origin POST fetch slice is
[native-engine-browser-053](tasks/native-engine-browser-053.md). Explicit and
initial page scripts can now issue bounded string-body POST requests with an
optional `Content-Type`, resolve response text/JSON promises, and commit
callback mutations through the content process. Redirect method rewriting and
the existing CSP, mixed-content, cookie, referrer, size, and CORS boundaries
remain enforced. Cross-origin preflight/simple-POST coverage, custom headers,
multipart/FormData/blob/stream bodies, XHR/WebSocket, and full Fetch Web IDL
identity remain open.

The completed bounded CORS preflight slice is
[native-engine-browser-054](tasks/native-engine-browser-054.md). Cross-origin
simple POSTs now use the direct Origin/response-CORS path, while non-simple
POSTs perform a bounded OPTIONS preflight that validates the authorized origin,
method, and `content-type` header before sending the request. Preflight cache,
custom headers, private-network access, opaque `no-cors` responses,
multipart/FormData/blob/stream bodies, XHR/WebSocket, and full Fetch Web IDL
identity remain open.

The completed bounded XHR bridge is
[native-engine-browser-055](tasks/native-engine-browser-055.md). The persistent
page realm now exposes asynchronous `XMLHttpRequest` GET/POST with string
bodies, the supported `Content-Type` header, bounded response status/text/URL/
header access, and `readystatechange`/`load`/`error` callbacks routed through
the existing fetch and CORS owner. Synchronous XHR, upload/progress,
binary-response, timeout/abort, streaming, WebSocket/EventSource, and full Web
IDL identity remain open.

The completed bounded text FormData slice is
[native-engine-browser-056](tasks/native-engine-browser-056.md). `fetch()` and
XHR now accept string-only `FormData`, serialize bounded deterministic
multipart bodies, and generate the matching boundary-bearing `Content-Type`;
the existing network and CORS policy remains the sole request owner. File/blob
parts, file chooser/upload progress, streaming, URLSearchParams, and full
FormData/Web IDL iterator identity remain open.

The completed bounded URLSearchParams slice is
[native-engine-browser-057](tasks/native-engine-browser-057.md). `fetch()` and
XHR now accept string-only `URLSearchParams`, serialize bounded URL-encoded
POST bodies with `+` spaces and the matching charset-bearing content type, and
retain the existing network/CORS owner. Full constructor, sorting, iterator,
streaming, and Web IDL identity remain open.

The completed bounded temporal-validation slice is
[native-engine-browser-058](tasks/native-engine-browser-058.md). Local and
child-owned forms now strictly validate `date`, `month`, `time`, and
`datetime-local` values, including calendar validity and bounded `min`/`max`/
`step` checks in the correct temporal units. Pattern/file constraints, custom
validity, and full `ValidityState` Web IDL identity remain open at that
checkpoint; custom validity is covered by the later 059 API slice.

The completed bounded form-validation API slice is
[native-engine-browser-059](tasks/native-engine-browser-059.md). Local and
child-owned controls now expose bounded `validity`, `validationMessage`, and
`willValidate` snapshots; `checkValidity()`/`reportValidity()` dispatch the
existing ordered `invalid` events; and `setCustomValidity()` persists through
the typed owner boundary. Pattern/file validation, picker/UI behavior, and
full live `ValidityState` Web IDL identity remain open.

The completed bounded FormData-constructor slice is a historical checkpoint
recorded in [native-engine-browser-060](tasks/native-engine-browser-060.md).
At that checkpoint, local and child-owned `new FormData(form)` collected
named, enabled text controls in document order, including controls associated
through an external `form` attribute, while submitter-only controls and
unchecked checkbox/radio controls were excluded. Its file-control rejection
boundary was superseded by the later 247 File/FileList and file-valued
FormData slice; full FormData Web IDL identity remains open.

The completed bounded pattern-validation slice is
[native-engine-browser-061](tasks/native-engine-browser-061.md). Local and
child-owned text-like controls now apply Rust-owned whole-value `pattern`
checks and expose `patternMismatch` through the existing validity API and
submission preflight. Invalid or unsupported regex syntax follows the HTML
invalid-pattern fallback and is ignored. Full JavaScript RegExp `v`-flag and
Unicode-set parity, file constraints, picker/UI behavior, and full live
`ValidityState` Web IDL identity remain open.

The completed bounded FormData select-control slice is
[native-engine-browser-062](tasks/native-engine-browser-062.md). Local and
child-owned `new FormData(form)` now preserve textarea values, selected
single-select values, and every initially selected enabled option of a
multi-select in document order. Interactive multi-select actions,
`optgroup` disabled inheritance, File/Blob parts, and full FormData Web IDL
identity remain open.

The completed bounded multi-select interaction slice is
[native-engine-browser-063](tasks/native-engine-browser-063.md). Local and
child-owned option clicks now toggle multiple selections, script
`option.selected` writes preserve them, `select.value` remains deterministic,
and the bounded host view exposes `multiple`, `options`, and
`selectedOptions`. Modifier-key/range selection, keyboard listbox behavior,
text selection/IME, `optgroup` disabled inheritance, and option-collection Web
IDL identity remain open.

The completed bounded semantic storage-contract slice is
[native-engine-browser-064](tasks/native-engine-browser-064.md). The native
dispatcher and `BrowserRuntimeSession` now execute bounded local/session
key-value read, write, and clear calls with active-context validation. The
state is backend-instance scoped and deliberately not page-visible, durable,
origin-keyed, cookie-synchronized, or IndexedDB-backed; cookie-scope requests
remain explicitly unsupported.

The completed bounded page Web Storage realm slice is
[native-engine-browser-065](tasks/native-engine-browser-065.md). The shared
QuickJS bootstrap now exposes bounded `localStorage` and `sessionStorage`
objects with `length`, `key`, `getItem`, `setItem`, `removeItem`, and `clear`
across local and child-owned evaluations. Realm-local persistence and
independent stores are covered; origin navigation persistence, durable
profiles, storage events, cookie synchronization, IndexedDB, and full Storage
Web IDL identity remain open.

The completed origin-keyed page Web Storage transfer slice is
[native-engine-browser-066](tasks/native-engine-browser-066.md). Bounded
local/session mutations are consumed by the runtime owner and carried into
fresh local realms and the sandboxed content worker. Tuple origins share their
state across navigation, while opaque local documents use a fragment-free
document key; the two stores remain independent. State is still volatile and
separate from the semantic `StorageRequest` maps; durable profiles, storage
events, cookie synchronization, IndexedDB, quota policy, and full Storage Web
IDL identity remain open.

The completed bounded text-backed File/Blob FormData slice is
[native-engine-browser-067](tasks/native-engine-browser-067.md). The shared
bootstrap exposes capped text-backed `Blob` and `File` values, supports their
`text()`/`slice()` metadata contract, accepts them in FormData `append`/`set`,
and emits filename/content-type multipart parts through the existing fetch
owner in local and child realms. Binary buffers, streams, file pickers, disk
file controls, upload progress, and full Blob/File/FormData Web IDL identity
remain open.

The completed opt-in durable page-storage profile slice is
[native-engine-browser-068](tasks/native-engine-browser-068.md). A bounded
JSON profile path now restores origin-keyed page `localStorage` for both the
local runtime and the sandboxed content worker, persists updates through
navigation and close, and deliberately starts each engine with empty
`sessionStorage`. The path is explicit rather than implicit, and concurrent
writers, storage events, IndexedDB, quota policy, and full Storage Web IDL
identity remain open.

The completed bounded `document.cookie` synchronization slice is
[native-engine-browser-069](tasks/native-engine-browser-069.md). Network page
realms can read non-HttpOnly session cookies and set bounded cookie lines; the
same Rust-owned jar controls subsequent navigation/fetch request headers,
while Secure, domain, path, expiry, and HttpOnly filtering remain enforced by
the transport owner. Storage events, cookie profile persistence, IndexedDB,
full binary/stream FormData support, and the remaining browser-context gates
are still open.

The `7ac4c39d` navigation-semantics correction repaired revision allocation,
nonfatal page-script exceptions, POST form handoff, hidden-action rejection,
timer-clock determinism, and lifecycle effect ordering; the native integration
gate was re-run at 359 passed before the next feature slice.

The completed bounded same-profile local storage-event slice is
[native-engine-browser-070](tasks/native-engine-browser-070.md). Concurrently
alive local native documents that share the same explicit profile path now
receive origin-filtered `storage` events for effective `localStorage` changes;
the source document is excluded, no-op writes/removes/clears are suppressed,
and the receiving Rust and JavaScript storage state is synchronized before
dispatch. The bounded coordinator is in-process only: session-storage
browsing-context routing, profile writer locking, IndexedDB, full
binary/stream FormData support, full task ordering/navigation edge cases,
remaining full pattern-regex/file constraint validation, target contexts, and
the remaining browser-context primitives remain open.

The completed process-backed local storage-event slice is
[native-engine-browser-071](tasks/native-engine-browser-071.md). Separate
sandboxed HTTP(S) content workers now report effective page `localStorage`
changes through bounded responses; the parent coordinator validates and fans
them out to other live engines sharing the explicit profile path, and the
receiving worker applies and dispatches them before its next page operation.
The source worker remains excluded and only local-storage events cross the
coordinator. Session-storage browsing-context routing, independent Glass
process/profile-writer coordination, IndexedDB, full binary/stream FormData
support, full task ordering/navigation edge cases, remaining full
pattern-regex/file constraint validation, target contexts, and the remaining
browser-context primitives remain open.

The completed bounded Web Storage profile-I/O locking slice is
[native-engine-browser-072](tasks/native-engine-browser-072.md). Profile
reads now use shared retained OS locks and complete profile snapshot writes
use exclusive retained locks, including the platform-specific commit fallback;
contended access returns a typed error after a bounded retry window, and the
Linux sandbox exposes the lock file to the worker. This serializes physical
profile I/O but does not merge stale independent full-state snapshots. Session-
storage browsing-context routing, cross-process event delivery, IndexedDB, full
binary/stream FormData support, full task ordering/navigation edge cases,
remaining full pattern-regex/file constraint validation, target contexts, and
the remaining browser-context primitives remain open.

The completed bounded session-storage routing slice is
[native-engine-browser-073](tasks/native-engine-browser-073.md). Every native
engine now has an explicit bounded browsing-context identity, the identity
crosses local and sandboxed worker realms, and `sessionStorage` event/state
delivery is restricted to other live engines representing that same context;
different contexts receive neither the event nor the mutation, and configured
backend responses no longer assume the historical `native-context` constant.
The coordinator remains in-process, session state remains volatile, and
cross-process events, stale-snapshot ownership or merge, IndexedDB, full
binary/stream FormData support, full task ordering/navigation edge cases,
remaining full pattern-regex/file constraint validation, target contexts, and
the remaining browser-context primitives remain open.

The completed bounded profile convergence slice is
[native-engine-browser-074](tasks/native-engine-browser-074.md). Profile
snapshots now carry a revision, and each locked write re-reads the latest
snapshot and applies the writer's bounded local-storage mutation deltas before
committing. Independent stale writers therefore retain unrelated local-storage
keys while preserving ordered last-writer behavior for the same key; legacy
profiles without a revision remain readable. Session storage is still volatile,
and this slice does not add cross-process event delivery, cookie profile
persistence, IndexedDB, or full browser parity.

The completed bounded cross-process storage-event slice is
[native-engine-browser-075](tasks/native-engine-browser-075.md). Each
profile-backed engine has a unique writer identity and a cursor into the
bounded newline-delimited event journal beside the profile. Local and session
storage changes are appended under the existing profile lock; live receivers
poll before page operations, exclude their writer, and apply origin and
browsing-context filters before dispatch. The journal repairs an incomplete
tail, rejects malformed records, and is capped at 4 MiB; acknowledged
records were initially retained until the bounded retention/recovery slice.

The completed bounded cookie-profile persistence slice is
[native-engine-browser-076](tasks/native-engine-browser-076.md). Supplying the
existing explicit profile path now restores accepted Rust-owned cookies for
page `document.cookie`, HTTP navigation/fetch, and sandboxed content workers;
profiles without the optional cookie field remain readable. Cookie snapshots
share the profile lock and atomic commit path, expire against bounded
wall-clock metadata, and merge key-level changes from stale live contexts.
Without a profile path cookies remain volatile. Full SameSite, partitioned,
third-party, Expires-date, cookie-change-event, and Cookie/Document Web IDL
parity remain open, as does IndexedDB.

The current bounded journal-retention and recovery slice is
[native-engine-browser-077](tasks/native-engine-browser-077.md). Profile-backed
engines register bounded `P.readers` leases beside the `P.events` journal;
heartbeats expire after 15 minutes, and healthy readers refresh at most every
30 seconds or when their cursor advances. Appends compact only a complete
prefix acknowledged by every live lease, shifting retained cursors under the
same `P.lock`; an active slow reader can still produce a typed 4 MiB limit
error. A missing/stale lease or out-of-range cursor reloads the authoritative
revisioned profile snapshot and sends a full bounded state replacement to a
sandboxed worker, so dropped event callbacks are not replayed.

The completed bounded IndexedDB persistence and transaction slice is
[native-engine-browser-078](tasks/native-engine-browser-078.md). Local and
sandboxed HTTP(S) content realms now share a bounded origin-keyed JSON
IndexedDB subset with version upgrades, object-store creation/deletion,
readonly/readwrite transactions, ordered request callbacks, key paths,
auto-increment keys, and bounded CRUD. The validated snapshot is persisted in
the existing profile and returned through the existing worker protocol; the
worker remains a profile-file consumer only through IPC. Indexes, cursors, key
ranges, non-JSON structured-clone values, full transaction/version-change
coordination, and cross-process IndexedDB journal/delta merging remained open
at that checkpoint.

The completed cross-process IndexedDB convergence slice is
[native-engine-browser-079](tasks/native-engine-browser-079.md). Local realms
and sandboxed content workers now publish bounded IndexedDB deltas beside
their Web Storage journal records. Live receivers apply those deltas to the
parent-owned origin state and deliver a bounded current-origin replacement to
the persistent realm; profile writes re-read the latest snapshot and merge
the same deltas under `P.lock`. The worker still never opens profile, journal,
or lease files, and full snapshots remain reserved for bounded runtime state
sync and reader-lease recovery. Disjoint live database writers are covered on
both local and content-worker paths. Same-database structural conflicts use
journal order as the explicit last-writer rule; indexes, cursors, key ranges,
non-JSON structured-clone values, full transaction/version-change
coordination, quota APIs, and the remaining browser-complete gates remain
open.

The completed bounded IndexedDB query-primitives slice is
[native-engine-browser-080](tasks/native-engine-browser-080.md). Object-store
indexes now persist bounded string key paths with unique and multi-entry
constraints; `IDBKeyRange` supports exact, lower, upper, and bounded queries;
and store/index lookups, counts, deletes, key cursors, value cursors, cursor
continuation, advancement, update, and delete are available in the supported
JSON key model. Index metadata uses a dedicated delta, while record changes
retain the existing merge path. Queries derive at most the bounded 128-record
scan at operation time, keeping resource use predictable at the cost of
large-database indexing throughput. Compound keys, array keys outside
`multiEntry`, non-JSON values, full transaction/version-change coordination,
quota APIs, and the remaining browser-complete gates remain open.

The completed bounded IndexedDB version-change coordination slice is
[native-engine-browser-081](tasks/native-engine-browser-081.md). A persistent
same-realm connection registry now dispatches `versionchange`, emits one
`blocked` notification while a prior connection remains open, and resumes the
pending upgrade after the final connection calls `close()`. The resumed
version-change transaction exposes its current object-store list and runs the
existing upgrade callbacks. Cross-process live connection identity, complete
`deleteDatabase` blocking, rollback, and the remaining browser-complete gates
remain open.

The completed bounded IndexedDB deletion-lifecycle slice is
[native-engine-browser-082](tasks/native-engine-browser-082.md). Same-realm
`deleteDatabase()` now dispatches `versionchange` with `newVersion: null`,
emits one `blocked` event while an earlier connection remains open, and
removes the database only after the final connection calls `close()`. Missing
deletes remain successful no-ops, and a subsequent open can recreate the
database. Cross-process live connection identity, full factory operation-queue
ordering, rollback, and the remaining browser-complete gates remain open.

The completed bounded IndexedDB transaction-rollback slice is
[native-engine-browser-083](tasks/native-engine-browser-083.md). Ordinary
write transactions now snapshot bounded JSON state, restore it on request
failure or explicit `abort()`, and deliver one `onabort` without a false
`oncomplete`. Concurrent transaction scheduling, upgrade-failure rollback,
structured-clone values, quota APIs, and the remaining browser-complete gates
remain open.

The completed bounded StorageManager quota slice is
[native-engine-browser-084](tasks/native-engine-browser-084.md).
`navigator.storage.estimate()` now reports deterministic JSON-size usage and
the fixed 4 MiB native profile quota, while `persist()` and `persisted()` are
stable asynchronous APIs that explicitly return `false` until a permission
policy exists. Quota prompts, reservation, cross-process arbitration,
structured-clone values, and the remaining browser-complete gates remain open.

The completed bounded IndexedDB transaction-serialization slice is
[native-engine-browser-085](tasks/native-engine-browser-085.md). Same-realm
transactions for one database now execute through an ordered queue, request
callbacks run before the next operation, and queued transactions snapshot
state only when they begin. This keeps rollback from erasing a predecessor’s
committed work. Cross-realm/process scheduling, upgrade-failure rollback,
structured-clone values, quota permission policy, and the remaining
browser-complete gates remain open.

The completed bounded IndexedDB structured-clone extension slice is
[native-engine-browser-086](tasks/native-engine-browser-086.md). The native
profile now retains tagged `undefined`, non-finite/negative-zero numbers,
`Date`, `RegExp`, `Map`, and `Set` values across reads and restart, with
bounded recursive encoding and explicit `DataCloneError` handling. Binary
buffers, typed arrays, Blob/File payloads, BigInt, and full clone parity remain
open.

The completed bounded IndexedDB Blob/File clone slice is
[native-engine-browser-087](tasks/native-engine-browser-087.md). Existing
text-backed native `Blob` and `File` values now retain payload, MIME type, and
file metadata across IndexedDB reads and profile restart. Byte-exact binary
buffers, typed arrays, streams, transfer lists, and full clone parity remained
open at that checkpoint.

The completed bounded IndexedDB binary structured-clone slice is
[native-engine-browser-088](tasks/native-engine-browser-088.md). `ArrayBuffer`,
common typed arrays, and `DataView` now persist as bounded byte-vector tags and
reconstruct after API reads and profile restart; `SharedArrayBuffer` fails
closed with `DataCloneError`. Transfer lists, detached-buffer identity,
binary Blob/File methods, streams, and full clone parity remained open at that
checkpoint.

The completed bounded Blob/File binary-read slice is
[native-engine-browser-089](tasks/native-engine-browser-089.md).
Text-backed `Blob` and `File` values now expose fresh UTF-8 bytes through
`arrayBuffer()` and `bytes()`, including deterministic Unicode and surrogate
handling, while the existing text-backed `size`, FormData, and persistence
contracts remain unchanged. Binary Blob construction, streams, transfer
semantics, and full Blob/File parity remain open.

The completed bounded fetch Blob/File-body slice is
[native-engine-browser-090](tasks/native-engine-browser-090.md). `fetch()` now
sends text-backed native `Blob` and `File` payloads directly and derives a
`Content-Type` from their normalized MIME type when no explicit header is
provided; the worker HTTP path is covered. XHR Blob bodies, binary Blob
construction, streams, abort, and full Fetch/Blob parity remain open.

The completed bounded XHR Blob/File-body slice is
[native-engine-browser-091](tasks/native-engine-browser-091.md). Asynchronous
XHR now forwards text-backed `Blob` and `File` bodies through the existing
fetch bridge with payload and MIME preservation; the worker HTTP path is
covered. Binary responses, upload progress, timeout/abort, streams, and full
XHR/Blob parity remain open.

The completed bounded fetch-abort slice is
[native-engine-browser-092](tasks/native-engine-browser-092.md). Native
`AbortController`/`AbortSignal` state, one abort event, reasons, and
observable `fetch()` promise rejection now work through the worker path; late
host responses are ignored. Socket-level cancellation, XHR `abort()`, timeout,
progress, `AbortSignal.timeout/any`, and full Web IDL parity remain open.

The completed bounded URLSearchParams slice is
[native-engine-browser-093](tasks/native-engine-browser-093.md).
`URLSearchParams` now accepts bounded query strings, pair sequences, records,
and existing instances; supports stable sort, size, callbacks, optional-value
deletion, and snapshot iterators; and uses the corrected form-urlencoded
escaping rules. Full live Web IDL iterator identity and exotic iterable parity
remain open.

The completed bounded fetch response-body slice is
[native-engine-browser-094](tasks/native-engine-browser-094.md). Native
responses now expose fresh text-backed `Blob`, UTF-8 `ArrayBuffer`, and
`Uint8Array` results through `blob()`, `arrayBuffer()`, and `bytes()` while
retaining independent `text()`/`json()` reads. Streaming response bodies and
byte-preserving non-UTF-8 response parity remain open.

The completed bounded fetch response-headers slice is
[native-engine-browser-095](tasks/native-engine-browser-095.md). `Response.headers`
now offers read-only case-insensitive content-type lookup plus bounded
`get`/`has`/iterator/`forEach` snapshots over the transferred header. Multiple
response headers, trailers, and full Headers/Web IDL parity remain open;
bounded request-header dictionaries are covered by the later 097 slice.

The completed bounded XHR-abort slice is
[native-engine-browser-096](tasks/native-engine-browser-096.md). Asynchronous
`XMLHttpRequest.abort()` now uses a request-local abort signal, resets active
requests to `UNSENT`, emits the bounded `readystatechange`/`abort` callbacks,
and ignores late `load`/`error` continuations. Transport cancellation,
timeout/progress, and complete XHR/Web IDL parity remain open.

The completed bounded Fetch request-header slice is
[native-engine-browser-097](tasks/native-engine-browser-097.md). Fetch now
accepts bounded plain-object custom request headers with early and Rust-side
validation, forbidden/internal-header rejection, same-origin wire delivery,
and sorted multi-header CORS preflight authorization. Full `Headers`
constructor/identity and mutation parity, duplicate-value list semantics,
response-header exposure, and trailers remain open.

The completed bounded byte-preserving response slice is
[native-engine-browser-098](tasks/native-engine-browser-098.md). Native Fetch
now carries a bounded raw-byte payload so `Response.blob()`,
`arrayBuffer()`, `bytes()`, and binary `Blob.slice()` preserve non-UTF-8
bytes, while `text()`/`json()` retain replacement-decoded text semantics.
Streaming/BYOB, transfer identity, binary Blob/File construction, and
binary/stream FormData parity remain open.

The completed bounded binary Blob/File request-body slice is
[native-engine-browser-099](tasks/native-engine-browser-099.md). Fetch and
asynchronous XHR now preserve raw bytes for Blob/File values that already
carry a bounded byte snapshot, while text-backed bodies, MIME propagation,
headers, redirects, and abort behavior remain unchanged. Binary Blob/File
construction, multipart FormData byte parity, streaming, upload progress,
and full Fetch/XHR/Blob Web IDL parity remain open.

The completed bounded binary Blob/File construction slice is
[native-engine-browser-100](tasks/native-engine-browser-100.md). Native
`Blob` and `File` now accept bounded `ArrayBuffer` and typed-array parts,
retain exact bytes and byte length, and feed the 099 Fetch/XHR request-body
bridge; text-only parts keep their established behavior. Streaming multipart
FormData, streams, upload progress, and full Blob/File Web IDL parity remain
open.

The completed bounded response-header slice is
[native-engine-browser-101](tasks/native-engine-browser-101.md). Fetch and
asynchronous XHR now expose bounded normalized response-header snapshots,
combine duplicate names, and apply same-origin/CORS-exposed filtering;
`Set-Cookie`, invalid raw header bytes, trailers, mutation, and full Headers
Web IDL parity remain open.

The completed bounded binary FormData-part slice is
[native-engine-browser-102](tasks/native-engine-browser-102.md). Multipart
Fetch and asynchronous XHR now preserve raw-byte-backed Blob/File parts while
retaining the existing boundary, filename, MIME, and text-field contracts;
streaming FormData, upload progress, iterator identity, and full FormData/Blob
Web IDL parity remain open.

The completed bounded Fetch `Headers` init/mutation slice is
[native-engine-browser-103](tasks/native-engine-browser-103.md). Native Fetch
now accepts bounded `Headers` records, pair sequences, and native `Headers`
instances with case-insensitive append/set/delete/get/iteration behavior;
response headers remain read-only snapshots, and full Headers Web IDL identity
and exotic iterable parity remain open.

The completed bounded XHR binary-response slice is
[native-engine-browser-104](tasks/native-engine-browser-104.md). Async XHR
now supports bounded `arraybuffer` and `blob` response types with byte-
preserving `response` values while retaining text-mode behavior.

The completed bounded XHR-timeout slice is
[native-engine-browser-105](tasks/native-engine-browser-105.md). Async XHR
now carries a bounded non-zero timeout through the existing request bridge;
zero disables the extra deadline, and a timed-out request reports `DONE`,
status zero, cleared response state, and `ontimeout` with stale completion
suppressed. Upload progress, transport cancellation beyond the bounded
deadline, streaming, synchronous XHR, and complete XHR Web IDL parity remain
open.

The completed bounded CORS-preflight-cache slice is
[native-engine-browser-106](tasks/native-engine-browser-106.md). Successful
preflights with a positive `Access-Control-Max-Age` are cached by document
origin, target, method, credentials mode, and sorted requested headers within
a bounded 64-entry/10-minute budget; failed, invalid, and zero-age responses
are not cached. Private-network access, opaque `no-cors` responses, and full
Fetch/CORS Web IDL parity remain open.

The completed bounded FormData-iterator slice is
[native-engine-browser-107](tasks/native-engine-browser-107.md). FormData now
exposes deterministic snapshot iterators for `entries()`, `keys()`, `values()`,
and `[Symbol.iterator]()` with self-iterating `next()` results, while
`forEach()` and multipart serialization retain the existing ordered text and
Blob/File entry owner. Live mutation during iteration, exotic iterables, and
complete FormData/Web IDL parity remain open.

The completed bounded URLSearchParams-iterable slice is
[native-engine-browser-108](tasks/native-engine-browser-108.md). The
constructor now accepts bounded Map, Set, and other pair-iterable inputs as
two-value sequences while retaining insertion order and the existing mutation,
sorting, encoding, and snapshot-iterator owners. Live iterator mutation/
identity and complete URLSearchParams Web IDL parity remain open.

The completed bounded Fetch-mode slice is
[native-engine-browser-109](tasks/native-engine-browser-109.md). Fetch now
defaults to `cors`, supports bounded `cors`, `no-cors`, and `same-origin`
policy, rejects cross-origin `same-origin` targets and non-safelisted
cross-origin `no-cors` request headers/content types before network I/O, and
projects successful cross-origin `no-cors` requests as opaque responses with
status zero, an empty URL/header view, and rejected body reads. Service-worker
and private-network integration, streaming, redirect parity, and complete
Fetch/Response Web IDL parity remain open.

The completed bounded Fetch-redirect slice is
[native-engine-browser-110](tasks/native-engine-browser-110.md). Fetch now
accepts `redirect: "follow" | "error" | "manual"`; follow reports a bounded
`redirected` flag and final URL, error rejects at the first redirect, manual
returns a filtered `opaqueredirect` response, and same-origin mode rejects
cross-origin redirect hops. Full redirect-status/referrer parity,
service-worker/private-network routing, streaming, and complete
Fetch/Response Web IDL parity remain open.

The completed bounded AbortSignal-combinator slice is
[native-engine-browser-111](tasks/native-engine-browser-111.md).
`AbortSignal.timeout()` now schedules one bounded `TimeoutError` abort on a due
host turn, while `AbortSignal.any()` composes a bounded iterable of native
signals with first-reason propagation, empty-input non-abortion, and listener
cleanup. Transport cancellation, XHR integration, and complete AbortSignal/Web
IDL parity remain open.

The completed bounded live-iterator slice is
[native-engine-browser-112](tasks/native-engine-browser-112.md).
URLSearchParams `entries()`, `keys()`, and `values()` now retain their owner,
observe bounded later mutations, and return self-iterating cursors, while
`[Symbol.iterator]` remains the `entries` method and existing encoding/body
owners stay unchanged. Full Web IDL descriptor parity and complex deletion or
reordering mutation semantics remain open.

The completed bounded static-abort slice is
[native-engine-browser-113](tasks/native-engine-browser-113.md).
`AbortSignal.abort(reason)` now creates an already-aborted signal with the
default or supplied reason through the existing signal consumers, without
allocating a timer or dispatching a post-construction event. Transport
cancellation, XHR integration, and complete AbortSignal/Web IDL parity remain
open.

The completed bounded live-FormData-iterator slice is
[native-engine-browser-114](tasks/native-engine-browser-114.md). FormData
`entries()`, `keys()`, and `values()` now retain their owner, observe bounded
later mutations, and return self-iterating cursors while multipart serialization
and `forEach()` stay on their existing owners. Complex deletion/reordering
semantics and complete FormData/Web IDL parity remain open.

The completed bounded live-request-Headers slice is
[native-engine-browser-115](tasks/native-engine-browser-115.md). Mutable
request `Headers` `entries()`, `keys()`, and `values()` now retain their owner,
observe bounded `append()`/`set()` mutations, and return self-iterating cursors;
immutable response-header views remain bounded snapshots. Full Headers Web IDL
parity, raw response headers, and trailers remain open.

The completed bounded Fetch Response-stream slice is
[native-engine-browser-116](tasks/native-engine-browser-116.md). Ordinary
responses now expose a bounded one-chunk native `ReadableStream` body with
`instanceof`, default-reader completion, lock/release, cancel, and async-
iterator hooks, while opaque and `opaqueredirect` responses keep `body ===
null` and existing filtered body-method failures. Progressive transport
streaming, backpressure, body disturbance/`bodyUsed`, BYOB readers,
transport-level cancellation, trailers, and complete ReadableStream/Response
Web IDL parity remain open.

The completed bounded Fetch Response-clone slice is
[native-engine-browser-117](tasks/native-engine-browser-117.md). Native
`Response.clone()` now returns a fresh bounded response with independent
headers and body owners; ordinary clones preserve the existing body readers
and one-chunk stream, while opaque and `opaqueredirect` clones retain their
filtered shells and `body === null`. Full body disturbance/`bodyUsed`, clone
rejection for locked or consumed bodies, shared tee/backpressure semantics,
Request/Response constructors, and complete Response Web IDL parity remain
open.

The completed bounded Fetch Request-object slice is
[native-engine-browser-118](tasks/native-engine-browser-118.md). Bounded
`Request` construction, cloning, and `fetch(request, overrides)` now reuse the
existing GET/POST, header, mode, redirect, credentials, body, signal, CORS,
abort, and transport owners. Full Request body streams, disturbance rules,
URL/cache/referrer/integrity fields, duplex, and complete Request/Headers Web
IDL parity remain open.

The completed bounded URL-object slice is
[native-engine-browser-119](tasks/native-engine-browser-119.md). HTTP(S)-focused
`URL` construction, relative path/query/fragment resolution, component
inspection, and snapshot `searchParams` now feed bounded Request and Fetch URL
inputs. URL setters, full percent-encoding/IDNA/IPv6/default-port parity, live
search-parameter synchronization, non-HTTP scheme parity, and complete
URL/Web IDL identity remain open.

The completed bounded live-URL-search-parameter slice is
[native-engine-browser-122](tasks/native-engine-browser-122.md). URL
`searchParams` owners now synchronize bounded `append()`, `set()`, `delete()`,
and `sort()` mutations to `search`/`href`; bounded `search` and `hash`
assignment updates the same URL owner, and Request/Fetch handoff observes the
current href. Full URL setter/parser and encoding parity, default-port/
IDNA/IPv6 behavior, and complete URL/Web IDL identity remain open.

The completed bounded URL-component-setter slice is
[native-engine-browser-123](tasks/native-engine-browser-123.md). Bounded
`pathname` normalization and `href` replacement now refresh the same URL and
`searchParams` owners while retaining query/fragment synchronization and
Request/Fetch handoff. Authority/protocol setters, complete parser/encoding/
IDNA/IPv6/default-port behavior, and complete URL/Web IDL identity remain open.

The completed bounded URL-authority-setter slice is
[native-engine-browser-124](tasks/native-engine-browser-124.md). HTTP(S) URL
`protocol`, `host`, `hostname`, `port`, `username`, and `password` setters now
rebuild the same URL owner, refresh origin/href, retain path/query/fragment
state, and preserve the live `searchParams` owner. Full URL encoding, IDNA,
IPv6/default-port canonicalization, non-HTTP schemes, and complete URL/Web IDL
parity remain open.

The completed bounded live-location-navigation slice is
[native-engine-browser-125](tasks/native-engine-browser-125.md). The native
JavaScript realm now exposes a frozen live `location` projection with bounded
`assign()`, `replace()`, `reload()`, `href`, component setters, and
`toString()`; local and content-process script commands route through the
existing Rust loader/commit owner, and `replace()` updates the current history
entry without adding one. At that checkpoint, publication/lifecycle
re-entrant navigation, nested contexts, full Location/Web IDL parity, and
complete URL parsing remained open; the publication subset is covered by the
follow-up below.

The completed bounded page-publication-navigation slice is
[native-engine-browser-126](tasks/native-engine-browser-126.md). Initial local
and content-process page-script phases, including `DOMContentLoaded`, `load`,
and `pageshow` publication, can return one typed `location` handoff to the
Rust navigation owner. Local and HTTP(S) content navigation follows bounded
handoffs, preserves `assign()`/`replace()` history semantics, and rejects
malformed or multiple handoffs. Outgoing `beforeunload`/`pagehide`/`unload`,
`hashchange`, nested contexts, full Location/Web IDL parity, and complete URL
parsing remain open.

The completed bounded native-inspection and CSS-target bridge is
[native-engine-browser-127](tasks/native-engine-browser-127.md). Native
runtime sessions now publish the engine-owned semantic-node projection and
PNG capture, while native one-shot dispatch maps `evaluate`, `click-at`,
`key`, `scroll`, `dom`, and PNG `screenshot` onto native owners. `css=` action
locators reuse the stylesheet selector grammar and reject malformed, missing,
or ambiguous matches before mutation. Full CSS selector/Web IDL parity,
form-value and semantic-region observation, revision guards, automatic native
selection, and production replacement of CDP remain open; the native feature
is still explicit-only at this checkpoint.

The completed bounded native form-control action batch is
[native-engine-browser-128](tasks/native-engine-browser-128.md). The stable
semantic action contract now carries clear, check, uncheck, and exact select
intents; Chromium maps them to the existing session actions, while the native
engine owns local and process-backed mutations with one parent revision. The
native one-shot CLI exposes the four commands, and Firefox/Safari/proof reject
the new actions explicitly. Full form semantics, rich observation, automatic
native selection, and production replacement of CDP remain open; native is
still explicit-only at this checkpoint.

The completed bounded native keyboard-action batch is
[native-engine-browser-129](tasks/native-engine-browser-129.md). The stable
semantic action contract now carries key-down, key-up, and modifier-aware
shortcut intents. Native local and HTTP(S) content-process documents dispatch
the same bounded DOM key events, expose key/code/modifier fields, honor
cancellation before default text editing, and publish one revision per action.
Chromium maps the new intents to its existing keyboard methods; other partial
adapters reject them explicitly. Selection/caret state, IME/composition,
repeat, text services, rich observation, automatic native selection, and
production replacement of CDP remain issue #40 gates.

The completed bounded native text-selection batch is
[native-engine-browser-130](tasks/native-engine-browser-130.md). Focused
`input` and `textarea` controls now retain bounded caret/range state through
local and HTTP(S) content-process document commits. The JavaScript host
exposes selection offsets, direction, `setSelectionRange`, and `select`; native
shortcuts support Ctrl/Meta+A, range replacement/deletion, and bounded
left/right/Home/End movement with Shift extension. Grapheme/bidi geometry,
clipboard, IME/composition, repeat, rich observation, automatic native
selection, and production replacement of CDP remain issue #40 gates.

The completed bounded native public-session revision batch is
[native-engine-browser-131](tasks/native-engine-browser-131.md). The portable
`BrowserRuntimeSession` now serializes guarded navigation and semantic action
dispatch, returns the shared typed stale-revision error, and accepts native
CLI `--expected-revision` values for navigation, form, keyboard, and scroll
commands. This removes the one-shot revision gap; normal `BrowserSession` /
TUI routing, the rest of the browser profile, native promotion, and CDP
replacement remain issue #40 gates.

The completed bounded native MCP routing batch is
[native-engine-browser-132](tasks/native-engine-browser-132.md). Native MCP
now owns a separate lazy `BrowserRuntimeSession` slot and routes core
navigation, evidence, semantic actions, script, PNG capture, targets, and
storage tools through the native backend without creating Chromium or falling
through to `BrowserSession`. Unsupported richer MCP tools return an explicit
error, while offline tools and the existing Chromium path retain their
behavior. Universal MCP workflow parity, TUI/profile/frame/download/prompt
owners, native promotion, and CDP replacement remain issue #40 gates.

The completed bounded native target-preflight batch is
[native-engine-browser-133](tasks/native-engine-browser-133.md). Native
engine, session, CLI, and MCP callers can now resolve semantic locators
against the current revision and receive the semantic node, visible viewport
geometry, actionability, resolution failure kind, and navigation/form hints
without scrolling, focusing, event dispatch, or storage mutation. The
existing Chromium preflight path remains unchanged. Universal workflow
parity, native promotion, and CDP replacement remain issue #40 gates.

The completed bounded native agent-inspection batch is
[native-engine-browser-134](tasks/native-engine-browser-134.md). Native
`inspectPage` and `findTarget` now build the standard Glass semantic result
envelopes from one revision-consistent page/node/layout snapshot. The native
intent path reuses Glass's pure resolver for constraints, confidence,
ambiguity, revision, and candidate fingerprints; CLI, MCP, and the portable
session expose the same discovery loop without creating Chromium. Rich
landmark regions, structured extraction, TUI, and CDP replacement remain
issue #40 gates.

The completed bounded native synchronization batch is
[native-engine-browser-135](tasks/native-engine-browser-135.md). Native
`wait` and `verify` now use bounded polling over the native semantic
observation and target-preflight surfaces, with standard Glass outcomes,
timeouts, composed predicates, target stability, and evaluate-policy gating
through the runtime session, CLI, and MCP. Request lifecycle accounting,
popup/dialog/download topology, action-specific `act-and-verify`, universal
workflow parity, and CDP replacement remain issue #40 gates.

The completed bounded native guarded-action batch is
[native-engine-browser-136](tasks/native-engine-browser-136.md). Native
`actAndVerify` now resolves and dispatches supported semantic intent actions
under one session lock, emits the standard execution/action envelopes with
revision evidence, and verifies the optional postcondition after dispatch
through CLI and MCP. Specialized request, popup, dialog, and download
witnesses, universal workflow parity, and CDP replacement remain issue #40
gates.

The completed native semantic-storage batch is
[native-engine-browser-137](tasks/native-engine-browser-137.md). Native
semantic localStorage and sessionStorage reads, writes, and clears now route
through the engine-owned page realm, so backend results and page JavaScript
observe the same origin-keyed state across local and content-process paths.
Cookie reads and clears are now covered by the native cookie profile surface;
the metadata-free semantic cookie write remains rejected. Target/frame
ownership, request-ledger, popup/dialog/download witnesses, universal workflow
parity, and CDP replacement remain issue #40 gates.

The completed native cookie-profile batch is
[native-engine-browser-138](tasks/native-engine-browser-138.md). Native cookie
inspection, metadata-preserving import, and clear operations now flow through
the parent/content-process owners and are exposed by the native runtime, CLI,
and MCP surfaces without CDP. The metadata-free semantic cookie write remains
rejected; multi-target/frame ownership, request accounting,
popup/dialog/download witnesses, universal workflow parity, and CDP
replacement remain issue #40 gates.

The completed native history/topology batch is
[native-engine-browser-139](tasks/native-engine-browser-139.md). Native async
back/forward traversal now uses the existing runtime worker and content
process for local and HTTP(S) history entries. The native runtime, CLI, and
MCP expose the standard bounded target/frame projections, with explicit
`native-context` and `native-context:main` identities and fail-closed
selection. Multi-target creation/closure, child-frame execution,
popup/dialog/download witnesses, request accounting, universal workflow
parity, and CDP replacement remain issue #40 gates.

The completed native prompt-lifecycle batch is
[native-engine-browser-140](tasks/native-engine-browser-140.md). Native page
JavaScript can surface bounded `alert`, `confirm`, and `prompt` metadata from
local realms, page-load/lifecycle scripts, and the external content worker;
the runtime, CLI, and MCP expose `dialogOpen`, `acceptDialog`, and
`dismissDialog` through the native owner without CDP. The current bridge uses
deterministic `false`/`null` script results while it records prompt state;
suspended modal continuation and caller-supplied prompt response injection
remain separate issue #40 gates, alongside target/frame expansion,
popup/download witnesses, request accounting, universal workflow parity, and
CDP replacement.

The completed native request-lifecycle batch is
[native-engine-browser-141](tasks/native-engine-browser-141.md). Native
navigation loads, direct fetches, and external content-process evaluations now
advance a bounded parent-owned request ledger; native `network-quiet` waits
use that ledger through the runtime, CLI, and MCP native seams and report
in-flight, quiet-duration, and completion-sequence state. This is operation-
boundary accounting, not yet a per-resource event stream for every
subresource, redirect, service worker, or transport cancellation; those
browser-complete contracts remain issue #40 work alongside continuation-aware
prompts, target/frame expansion, popup/download witnesses, universal workflow
parity, and CDP replacement.

The completed native external-link activation batch is
[native-engine-browser-142](tasks/native-engine-browser-142.md). Direct
semantic clicks on anchors in external HTTP(S) documents now stay in the
content-process event path, honor click cancellation, and hand an allowed link
to the asynchronous native navigation owner. The action revision records the
click and the navigation separately, preserving the existing lifecycle,
history, request-ledger, and no-CDP guarantees. Download targets, target/frame
expansion, and the remaining browser-complete workflow gates remain issue #40
work.

The completed native-engine-browser-143 batch is
[native-engine-browser-143](tasks/native-engine-browser-143.md). Native
`download` anchors now remain on the committed page while an allowed click or
script navigation queues a parent-owned transfer. The navigation-mode loader
fetches bounded cross-origin HTTP(S) bytes, and the native runtime, CLI, and
MCP complete those bytes into an existing directory with sanitized,
collision-free names, SHA-256 evidence, stable completion IDs, and bounded
cancel/list bookkeeping. Non-download external links retain the 142
asynchronous navigation path. Popup/new-target behavior, child-frame
ownership, chooser/programmatic downloads, universal workflow parity, and
native production promotion remain issue #40 work.

The completed native-engine-browser-144 batch is
[native-engine-browser-144](tasks/native-engine-browser-144.md). Native
sessions now own a bounded registry of up to 32 independent page engines.
Create, select, list, and close operations preserve target-local document,
history, content-worker, request, prompt, and download state; projections
include stable IDs, opener linkage, redacted URL/title evidence, and one
explicit active target. Runtime, CLI, and MCP route the lifecycle without
Chromium or CDP, and session close drains parked and active engines. Child
browsing contexts, popup-default-action creation, and frame-scoped operations
remain the next topology gates rather than being inferred from target support.

The completed native-engine-browser-145 batch is
[native-engine-browser-145](tasks/native-engine-browser-145.md). Native
targets now own a bounded nested frame tree with stable `target:main` and
`target:frame-N` identities, parent linkage, `src`/`srcdoc` discovery, and
initialized child engine owners. Explicit frame selection swaps complete
document/realm/history state, so normal navigation, evidence, script, action,
storage, prompt, download, wait, and capture routes operate on the selected
frame; parent and sibling state remains parked and recoverable. Frame close
and session close drain every child owner. CSP frame-source enforcement,
frame event/load parity, shared frame scripting, and complete browser topology
remain open issue #40 gates.

The completed native-engine-browser-146 batch is
[native-engine-browser-146](tasks/native-engine-browser-146.md). Allowed
activation of an anchor with `target="_blank"` now keeps the opener committed,
creates an initialized parked native page target with opener linkage, and
routes through the same behavior for local documents, external content-worker
documents, and page-script `element.click()`. Runtime, CLI, and MCP expose
revision-safe `clickExpectPopup` evidence, while generic native actions and
scripts materialize the same target owner without CDP. Named browsing
contexts, `window.open`, popup permissions/geometry, shared opener scripting,
and complete browser topology remain issue #40 gates.

The completed native-engine-browser-147 batch is
[native-engine-browser-147](tasks/native-engine-browser-147.md). External
document responses now transfer their effective `frame-src`, `child-src`, or
`default-src` source list through the versioned content-worker boundary. The
parent frame registry evaluates that policy before child initialization;
blocked requests publish live `about:blank` owners without fetching the
blocked URL, while allowed same-origin frames retain the normal sandboxed
content-worker path. Existing frame owners retain the embedding policy across
direct navigation, link activation, redirects, and history traversal. Frame
event/load parity, shared frame scripting, full source-expression grammar,
and complete browser topology remain issue #40 gates.

The completed native-engine-browser-148 batch is
[native-engine-browser-148](tasks/native-engine-browser-148.md). Native page
realms now expose `window.open(url, target)` through a typed host effect. Local
and HTTP(S) pages create initialized parked targets without CDP, reserved
same-context names use the existing navigation owner, and non-reserved names
reuse the same target on later calls. Popup intents are transferred through
version-5 content-worker IPC, including page-load, direct-script, event,
lifecycle, fetch-continuation, and nested-target paths; bounded popup cascades
are materialized iteratively while the opener remains selected. Cross-context
WindowProxy property scripting, popup permission policy, and geometry remain
later issue #40 gates.

The completed native-engine-browser-149 batch is
[native-engine-browser-149](tasks/native-engine-browser-149.md). Native
WindowProxy handles now carry bounded `postMessage` effects across local and
HTTP(S) page realms, including the content-worker boundary. The parent target
registry resolves private handles, named targets, and direct context IDs;
delivered events expose cloned data, serialized origin, a source proxy, and
bounded `targetOrigin` filtering. Replies through `event.source`, page-load and
script effects, protocol-6 transfer, and nested-effect draining all use the
same native owner without CDP. Direct cross-context property scripting,
`window.opener`/mutable `window.name`, popup permission policy and geometry,
frame lifecycle/scripting, and complete browser parity remain issue #40 gates.

The completed native-engine-browser-150 batch is
[native-engine-browser-150](tasks/native-engine-browser-150.md). Native
browsing contexts now carry bounded `window.name` and opener metadata through
local and HTTP(S) realms, including content-worker initialization. Script can
read `window.opener` and its opener name, mutate its own name, send opener
messages, and cause later named-target calls to reuse the renamed target. The
opener projection is intentionally snapshotted at child creation; live
cross-context property scripting, popup permission policy and geometry, frame
lifecycle/scripting, and complete browser parity remain issue #40 gates.

The completed native-engine-browser-151 batch is
[native-engine-browser-151](tasks/native-engine-browser-151.md). Native
WindowProxy `close()` now emits a bounded parent-owned effect across local and
HTTP(S) realms, including the content-worker boundary. The target registry
resolves private handles, direct context IDs, and named targets, processes
close effects after any causally-required popup creation, and routes shutdown
through the existing target/frame/worker owner. The requesting proxy reports
`closed === true`, closed targets leave topology, and stale repeats are
idempotent. Live updates to every already-held proxy remain a separate
identity-observation gate in issue #40.

The completed native-engine-browser-152 batch is
[native-engine-browser-152](tasks/native-engine-browser-152.md). Native
WindowProxy `location` now exposes a bounded URL snapshot and parent-owned
`href`, `assign()`, `replace()`, and `reload()` navigation effects across local
and HTTP(S) realms, including the content-worker boundary. The target registry
resolves direct context IDs, source-owned private handles, and names, applies
normal URL resolution through the target's current URL, and routes navigation
through the existing history/frame/storage owner. Child realms also receive a
validated opener URL snapshot. Live updates to every extant proxy,
popup permission/geometry, frame lifecycle/shared scripting, and complete
browser parity remain issue #40 gates.

The completed native-engine-browser-153 batch is
[native-engine-browser-153](tasks/native-engine-browser-153.md). Already-held
WindowProxy objects now receive bounded parent-owned refreshes before
observable work, so location and `closed` state tracks later target navigation,
renaming, and closure across local and HTTP(S) realms. Private handles are
resolved without falling through to an unrelated named target after closure;
closed-target tombstones are bounded and content-worker realms receive the
same refresh through a validated synchronization message. Full live Web IDL
identity, popup permission/geometry, frame lifecycle/shared scripting, and
complete browser parity remain issue #40 gates.

The completed native-engine-browser-154 batch is
[native-engine-browser-154](tasks/native-engine-browser-154.md). Local and
HTTP(S) lifecycle callbacks now return one validated navigation handoff from
`beforeunload`, `pagehide`, `unload`, `popstate`, or `hashchange` to the same
parent-owned loader/history/frame path. Cancellation remains distinct from a
completed lifecycle with no handoff; the already-dispatched outgoing lifecycle
is skipped exactly once for its own handoff, and bounded multiple/looping
handoffs fail closed. Native lifecycle and same-document callback re-entry are
now covered by 436 native integration tests. Popup permission/geometry, frame
lifecycle/shared scripting, complete Web IDL/browser topology, and native
default promotion remain issue #40 gates.

The completed native-engine-browser-155 slice is
[native-engine-browser-155](tasks/native-engine-browser-155.md). Local and
HTTP(S) page realms now expose bounded Window, Document, Node, Element,
HTML-element, Location, NodeList, HTMLCollection, Event, CustomEvent, and
StorageEvent identity, including current-document `ownerDocument` refreshes
and stable `window`/`self`/`parent`/`top`/`frames` relationships. This improves
ordinary feature detection and collection/event interoperability without
claiming full Web IDL descriptors, live tree mutation, shadow/custom elements,
ranges, cross-origin frame properties, shared frame scripting, or complete
browser parity. Popup permission/geometry, frame lifecycle/shared scripting,
complete Web IDL/browser topology, and native default promotion remain issue
#40 gates.

The completed native-engine-browser-156 slice is
[native-engine-browser-156](tasks/native-engine-browser-156.md). The
parent-owned frame registry now projects direct child `contentWindow` and
same-origin `contentDocument` relationships into page scripts, including
`window.frames`, `parent`, `top`, `frameElement`, bounded child selectors and
collections, origin-checked `postMessage`, and child WindowProxy navigation.
Child navigation replaces the child snapshot without reloading it from a
stale embedding source, and descendant owners are drained before future
topology discovery. Complete cross-realm identity, nested child-window
projection in every event path, popup permission/geometry, full frame
lifecycle, and complete browser parity remain issue #40 gates.

The completed native-engine-browser-157 slice is
[native-engine-browser-157](tasks/native-engine-browser-157.md). The recursive
parent-owned binding tree now lets an embedding page traverse same-origin
nested `contentWindow`/`contentDocument` relationships, including nested
`window.frames`, `length`, numeric child windows, `parent`, `top`, and
`frameElement` identity. Nested `postMessage()` and WindowProxy navigation use
the existing parked-descendant routes, and document/window caches refresh when
child revisions or descendant topology change. The transfer remains bounded
and snapshot-based; complete cross-origin Window behavior, live cross-realm
identity, full frame lifecycle, and complete browser parity remain issue #40
gates.

The completed native-engine-browser-158 slice is
[native-engine-browser-158](tasks/native-engine-browser-158.md). Selected child
realms now receive their actual frame ID plus bounded parent/top Window and
document descriptors through both local and content-worker paths. Parent
descriptors include the active selected child, so `window.parent.document`,
`defaultView`, `parent.frames[n]`, `frameElement`, and nested WindowProxy
identity resolve back to the selected global; frame-target routes accept frame
IDs while popup and target-owner effects retain target-context identity.
Cross-origin Window Web IDL,
full frame lifecycle/load ordering, live cross-realm identity, and complete
browser parity remain issue #40 gates.

The completed native-engine-browser-159 slice is
[native-engine-browser-159](tasks/native-engine-browser-159.md). Projected
cross-origin frame windows now enforce the caller-relative boundary: document
and bounded sensitive Window properties raise a typed `SecurityError`,
cross-origin `contentDocument` and `frameElement` remain inaccessible, and
selected cross-origin children report a null `window.frameElement`. Same-origin
frame projections retain their existing identity and navigation behavior, while
cached WindowProxy origin state is refreshed after target navigation. Complete
Window Web IDL, live cross-realm identity, full frame lifecycle/load ordering,
and native browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-160 slice is
[native-engine-browser-160](tasks/native-engine-browser-160.md). Same-origin
`contentDocument` projections now route bounded element focus, click, value,
selection, checked/selected state, validity, and attribute mutations to the
child frame's real native document owner; parent and selected-child writes are
covered through the HTTP frame harness. Routing validates source/target
origins and preserves the existing process boundary. Structural DOM creation,
event-listener identity across realms, full frame lifecycle/load ordering, and
native browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-161 slice is
[native-engine-browser-161](tasks/native-engine-browser-161.md). Script
`textContent` and `innerText` writes now replace the targeted native subtree,
detach replaced descendants from selectors/layout/frame discovery, and commit
through local, content-worker, and same-origin frame owners. Dynamic element
creation, live child-node collections, cross-realm listener identity, full
frame lifecycle/load ordering, and native browser-complete promotion remain
issue #40 gates.

The completed native-engine-browser-162 slice is
[native-engine-browser-162](tasks/native-engine-browser-162.md). Bounded
`innerHTML` replacement now parses and commits structural fragments, while
`remove()` and `removeChild()` detach existing subtrees. Canonical bounded
markup is exposed through the refreshed host projection, and the operations
are covered through local document, HTTP content-worker, and same-origin frame
owners. Dynamic element creation, live child-node collections, cross-realm
listener identity, full frame lifecycle/load ordering, and native
browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-163 slice is
[native-engine-browser-163](tasks/native-engine-browser-163.md). Local and
HTTP content-worker realms can now create Rust-owned elements and text nodes,
populate detached trees, move existing subtrees, and insert children at a
validated position through `appendChild()`/`insertBefore()`. Same-evaluation
parent identity and bounded markup are coherent, and the next evaluation
refreshes the committed snapshot. Same-origin frame construction, live
child-node collections, cross-realm listener identity, full frame
lifecycle/load ordering, and native browser-complete promotion remain issue
#40 gates.

The completed native-engine-browser-164 slice is
[native-engine-browser-164](tasks/native-engine-browser-164.md). Projected
same-origin frame documents now expose detached element/text factories and
group consecutive structural commands into one typed child-owner transaction.
Parent identity and markup remain coherent during the evaluation, and a fresh
frame snapshot exposes the committed nodes. Live child-node collections,
cross-origin restrictions, cross-realm listener identity, full frame
lifecycle/load ordering, and native browser-complete promotion remain issue
#40 gates.

The completed native-engine-browser-165 slice is
[native-engine-browser-165](tasks/native-engine-browser-165.md). Top-level and
same-origin frame realms now expose owner-backed live `NodeList`/`HTMLCollection`
views for `childNodes` and `children`, including live length, indexed access,
iteration, `item()`, and `namedItem()` behavior. Element and text hosts expose
child/sibling traversal, `hasChildNodes()`, `contains()`, `replaceChild()`, and
connectivity checks across insertion, removal, replacement, and subtree moves.
Full Web IDL descriptors, parser text-node identity, cross-realm listener
identity, complete frame lifecycle/load ordering, and native browser-complete
promotion remain issue #40 gates.

The completed native-engine-browser-166 slice adds attached text-node
snapshots. The script projection now preserves parsed text-node order and
identity across evaluations, including `nodeValue`, `data`, `parentNode`,
siblings, and live `childNodes`/`children` filtering in local, content-worker,
and same-origin frame realms. Rust remains the source of truth for the
attached tree and text mutation; full Web IDL descriptors, observer delivery,
and native browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-167 slice expands selector and class
mutation behavior across top-level and same-origin frame realms. Scoped
`querySelector()`/`querySelectorAll()`/`getElementsBy*()` now handle compound,
descendant, child, comma-list, attribute, and common state/structural
pseudo-class selectors; element `matches()` and `closest()` share the same
grammar. `classList` supports live token reads and add/remove/toggle/replace
mutation through typed attribute commands. Full CSS selector grammar, complete
Web IDL descriptors, cross-realm listener identity, and native browser-complete
promotion remain issue #40 gates.

The completed native-engine-browser-168 slice adds live bounded `style` and
`dataset` surfaces to local and same-origin frame element projections.
CSS-style declaration reads, camelCase/dashed property access,
`setProperty()`/`removeProperty()`, `cssText`, priorities, dataset camelCase
mapping, enumeration, writes, and deletion all route through the existing
native attribute command owner. Full CSSOM/value validation, computed-style
Web IDL parity, complete cross-process event delivery, and native
browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-169 slice adds frame-qualified EventTarget
behavior to same-origin projected elements, detached projected elements,
documents, and window proxies. Capture/bubble dispatch follows the projected
parent-node tree to the frame-local document and window, detached nodes stay
detached, listener removal is honored, and projected click/focus/blur retain
their typed child-command handoff. Complete cross-process event observation,
full Web IDL parity, and native browser-complete promotion remain issue #40
gates.

The completed native-engine-browser-170 slice adds cross-process event
observation for same-origin frame activity. Bounded child `NativeEffect`
metadata now crosses the backend boundary for frame commands and selected
frame navigation, actions, and scripts; the parent refreshes its binding,
resolves the child node/document/window target, and dispatches through the
frame-local event path. Parent-issued focus/blur/click preflight is not
replayed, and ancestor or cross-origin targets remain isolated. Full event
ordering across every lifecycle/input path, observer APIs, Web IDL parity, and
native browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-171 slice propagates typed runtime effects
from child-frame navigation and `postMessage` through validated same-origin
parent and ancestor projections. Lifecycle and hash-change targets now carry
explicit window metadata, parent-handler effects continue through the bounded
ancestor cascade, and nested frame scripts/browser queues remain observable.
Initial document load observer delivery, stale-generation rejection, complete
Web IDL parity, and native browser-complete promotion remain issue #40 gates.

The completed native-engine-browser-172 slice carries bounded page-load event
metadata across the content-worker boundary and records it at the receiving
document revision. Frame bindings now include document generations: stale
DOM/document effects are ignored after navigation while stable frame-window
lifecycle events remain observable. Complete observer APIs, resource
scheduling, cross-realm identity, Web IDL parity, and native browser-complete
promotion remain issue #40 gates.

The completed native-engine-browser-173 slice adds bounded `MutationObserver`
delivery to local and content-worker page realms. Script-representable
attribute, character-data, child-list, reparenting, removal, and text/HTML
replacement commands produce ordered records with optional old values and
subtree filtering, and one existing Promise-job checkpoint delivers a callback
batch. `disconnect()` and `takeRecords()` are supported. Cross-realm observer
projection, layout/resource observers, complete resource scheduling, Web IDL,
and browser-wide parity remain issue #40 gates.

The completed native-engine-browser-174 slice carries that observer contract
through same-origin `contentDocument` projections. Frame-qualified shadow state
prevents parent/child node-index aliasing; projected attribute,
character-data, child-list, reparenting, removal, and text/HTML replacement
commands produce records before `FrameScriptBatch` handoff, including detached
projected targets. Independent child-task effects, layout/resource observers,
complete resource scheduling, Web IDL, and browser-wide parity remain issue #40
gates.

The completed native-engine-browser-175 slice connects page-script geometry to
the Rust-owned layout snapshot. Local, process-backed, and projected
same-origin frame elements expose DOMRect, client/offset/scroll dimensions, and
bounded `ResizeObserver` records at the existing Promise-job checkpoint;
position-only root scrolling does not create resize records. Content-worker
scroll synchronization keeps process-backed `getBoundingClientRect()` values
aligned with the visible viewport. Layout/resource observer breadth, complete
resource scheduling, fractional geometry, Web IDL descriptor parity, and
browser-wide parity remain issue #40 gates.

The completed native-engine-browser-176 slice extends that host checkpoint with
bounded `requestAnimationFrame`/`cancelAnimationFrame`, navigation-scoped
`performance.now()`, and `IntersectionObserver`/`IntersectionObserverEntry`.
Initial and threshold-crossing intersection records use the same Rust-owned
layout and scroll snapshot in local and process-backed realms; cancelled frame
callbacks do not run. Independent rendering/vsync, fractional/composited
geometry, resource observers, complete scheduling, Web IDL descriptor parity,
and browser-wide parity remain issue #40 gates.

The completed native-engine-browser-177 slice carries the History API through
direct evaluation, local input/lifecycle dispatch, and process-backed HTTP(S)
event callbacks. Bounded structured state, same-origin URL resolution,
`history.length`, same-document traversal, popstate state, and worker URL/state
synchronization are now typed and tested across both execution boundaries;
ambiguous traversal plus competing navigation fails closed. Cross-document
session history, bfcache, cross-origin history, complete Web IDL descriptors,
and browser-wide parity remain issue #40 gates. Its contract and local evidence
are recorded in [native-engine-browser-177](tasks/native-engine-browser-177.md).

The completed native-engine-browser-178 slice adds document-fragment construction
and child-mutation ergonomics to local, HTTP(S) content-worker, and same-origin
frame realms. `createDocumentFragment()`, fragment identity and ownership,
fragment flattening, sibling insertion, replacement, and child replacement now
share the host-owned tree/cache/removal rules used by elements and text nodes;
empty-fragment synchronization also clears stale projected content. The slice
intentionally does not claim fragment `innerHTML` setters, direct
`MutationObserver` records for fragment staging, or complete Web IDL descriptors.
Its contract and local evidence are recorded in
[native-engine-browser-178](tasks/native-engine-browser-178.md); cross-document
session history, bfcache, cross-origin history, and browser-wide parity remain
issue #40 promotion gates.

The completed native-engine-browser-197 slice completes the bounded `Attr`
participation in the shared `Node` contract. Attribute nodes now expose the
standard detached node accessors, clone/equality behavior, empty child
collections, and detached root/connectivity semantics while preserving
`ownerElement` as their ownership link. Local, HTTP(S) content-worker, and
same-origin-frame paths share the implementation. Namespace-aware storage,
XML documents, complete Web IDL descriptors, and browser-wide conformance
remain issue #40 work. Exact evidence is recorded in
[native-engine-browser-197](tasks/native-engine-browser-197.md).

The completed native-engine-browser-198 slice adds a bounded CSS Grid layout
mode. `display: grid` now participates as a block-level container with typed
fixed/`fr`/auto track lists, bounded `repeat()` parsing, row-major element
placement, gaps, alignment, and shared layout/paint/hit-test geometry. The
contract intentionally leaves explicit placement, named lines, `minmax()`,
percentage and auto-repeat sizing, implicit tracks, full intrinsic sizing, and
anonymous text grid items for later slices. Exact evidence is recorded in
[native-engine-browser-198](tasks/native-engine-browser-198.md).

The completed native-engine-browser-199 slice adds bounded inline SVG shape
rendering. SVG intrinsic dimensions and nested `g` placement now feed shared
layout boxes; `rect`, `circle`, and `ellipse` fills feed the native display-list
and software capture path; and SVG boxes retain normal geometry and hit-test
identity. The contract intentionally leaves paths, lines, gradients, strokes,
transforms, viewBox mapping, text, external resources, and complete SVG
namespaces for later work. Exact evidence is recorded in
[native-engine-browser-199](tasks/native-engine-browser-199.md).

The completed native-engine-browser-200 slice preserves namespace identity
through the native DOM owner, content-worker wire, JavaScript snapshots,
mutation commands, and same-origin frame projections. Parsed SVG subtrees,
MathML subtrees, `foreignObject` HTML descendants, script-created
`createElementNS()` nodes, and namespace-aware fragment parsing now share the
same contract. Unsupported namespace URIs report `NamespaceError`; at that
checkpoint qualified namespace attributes, XML documents, and complete
namespace-specific Web IDL remained separate issue #40 work. Exact evidence is
recorded in [native-engine-browser-200](tasks/native-engine-browser-200.md).

The completed native-engine-browser-201 slice carries bounded
namespace-qualified attributes through the native owner, content-worker
snapshots, mutation commands, local/detached DOM, and same-origin frame
projections. XLink, XML, and XMLNS identity now survives parsing,
`get/set/removeAttributeNS`, live `Attr`/`NamedNodeMap` access, cloning, and
`createAttributeNS`; invalid qualified names and unsupported namespaces fail
with `NamespaceError`. XML documents, namespace-aware CSS selectors, complete
namespace-specific Web IDL, and browser-wide conformance remain issue #40
promotion work. Exact evidence is recorded in
[native-engine-browser-201](tasks/native-engine-browser-201.md).

The completed native-engine-browser-202 slice adds bounded SVG stroke paint to
the existing `rect`, `circle`, and `ellipse` layout boxes. A typed display-list
stroke command and shared software raster path now honor `stroke`, `none`,
inline-style declarations, `currentColor`, bounded numeric/`px` widths,
fill-less shapes, scroll translation, clipping, and alpha composition. Paths,
line caps/joins, transforms, viewBox mapping, gradients, markers, and external
SVG resources remain separate issue #40 promotion work. Exact evidence is
recorded in [native-engine-browser-202](tasks/native-engine-browser-202.md).

The completed native-engine-browser-203 slice adds bounded SVG `line`,
`polyline`, and `polygon` geometry. Shared point parsing now feeds layout
bounds, typed polygon-fill and polyline-stroke display commands, deterministic
software rasterization, clipping, scroll translation, alpha composition, and
hit-test ownership. Malformed or over-limit point lists fail closed. Paths,
explicit cap/join styles, dash arrays, transforms, viewBox mapping, gradients,
markers, and external SVG resources remain separate issue #40 promotion work.
Exact evidence is recorded in
[native-engine-browser-203](tasks/native-engine-browser-203.md).

The completed native-engine-browser-204 slice adds bounded straight SVG
`path` support for absolute/relative `M`, `L`, `H`, `V`, and `Z` commands.
Shared subpaths now feed layout bounds, typed path fill/stroke display
commands, even-odd filling, segment stroke coverage, clipping, scroll
translation, alpha composition, and hit-test ownership. Curves, arcs, dash
arrays, explicit cap/join styles, transforms, viewBox mapping, gradients,
markers, and external resources remain separate issue #40 promotion work.
Exact evidence is recorded in
[native-engine-browser-204](tasks/native-engine-browser-204.md).

The completed native-engine-browser-205 slice adds bounded quadratic and cubic
SVG path curves through absolute/relative `Q` and `C` commands. Fixed-count
curve flattening now feeds the existing shared subpath layout bounds, typed
path fill/stroke commands, clipping, scroll translation, alpha composition,
capture, and hit-test surfaces. Smooth/reflected commands, elliptical arcs,
dash arrays, explicit cap/join styles, transforms, viewBox mapping, gradients,
markers, and external SVG resources remain separate issue #40 promotion work.
Exact evidence is recorded in
[native-engine-browser-205](tasks/native-engine-browser-205.md).

The completed native-engine-browser-206 slice adds bounded smooth/reflected
`S`/`T` and elliptical-arc `A` SVG path commands. Reflected controls and
endpoint-to-center arc conversion are flattened into the shared subpath
representation with finite-input, invalid-flag, 64-sample-per-arc, and
2,048-point guards. Layout bounds, typed path fill/stroke paint, clipping,
scroll translation, alpha composition, capture, and hit-test ownership remain
shared. Adaptive curve error, viewBox mapping, dash arrays,
explicit cap/join styles, gradients, markers, and external resources remain
separate issue #40 promotion work. Exact evidence is recorded in
[native-engine-browser-206](tasks/native-engine-browser-206.md).

The completed native-engine-browser-207 slice adds bounded SVG transform
matrices for `matrix`, `translate`, `scale`, `rotate`, `skewX`, and `skewY`.
Root, group, and shape transforms compose through shared integer geometry so
transformed rectangles, ellipses, lines, polygons, and paths use the same
layout bounds, typed fill/stroke paint, clipping, scroll translation, alpha
composition, capture, and hit-test owners. Malformed, non-finite, unknown, and
over-limit transform output fails closed; identity transforms preserve the
existing command forms. CSS transforms, dash arrays, explicit
cap/join styles, gradients, markers, and external resources remain separate
issue #40 promotion work. Exact evidence is recorded in
[native-engine-browser-207](tasks/native-engine-browser-207.md).

The completed native-engine-browser-208 slice adds bounded SVG `viewBox` and
`preserveAspectRatio` mapping through the shared affine transform owner.
Default `xMidYMid meet`, nine bounded alignments, `meet`, `slice`, and
nonuniform `none` mappings convert SVG user space into the declared viewport;
the resulting points and path subpaths continue through shared layout bounds,
typed fill/stroke paint, clipping, scroll projection, alpha composition,
capture, and hit testing. Malformed viewBox/viewport data fails closed. CSS
sizing/percentages, nested viewport placement, dash
arrays, explicit cap/join styles, gradients, markers, and external resources
remain separate issue #40 promotion work. Exact evidence is recorded in
[native-engine-browser-208](tasks/native-engine-browser-208.md).

The completed native-engine-browser-209 slice adds SVG viewport clipping to
the shared half-open clip owner. SVG ancestor viewport rectangles now compose
with CSS overflow clips for projected bounds, software rasterization, scroll
translation, capture, and hit testing; clipped shapes cannot leak pixels or
win shape hit ownership outside their viewport. Layout retains document-space
geometry, and containing HTML boxes remain eligible where the existing hit-test
stack requires them. Rounded clip paths, nested SVG viewport placement,
clip-path/mask semantics, dash arrays, explicit cap/join styles, gradients,
markers, and external resources remain separate issue #40 promotion work.
Exact evidence is recorded in
[native-engine-browser-209](tasks/native-engine-browser-209.md).

The completed native-engine-browser-210 slice adds bounded inline PNG data-URL
images. Validated PNGs now participate in intrinsic/aspect-ratio inline sizing,
typed display-list paint, nearest-neighbor software replay, source-over alpha,
shared clipping, hit testing, root scrolling, and PNG capture. External image
fetching, decoded-resource transfer/caching, CSS image layers beyond the
single background-image URL, SVG image resources, and animated formats remain
active issue #40 browser-completeness work. Exact evidence is recorded in
[native-engine-browser-210](tasks/native-engine-browser-210.md).

The completed native-engine-browser-211 slice adds bounded external PNG image
resources. The content process now applies the existing HTTP(S), referrer,
cookie, redirect, mixed-content, and CSP `img-src` policy before fetching
static image sources; successful PNGs cross the typed document wire as
validated intrinsic dimensions and RGBA pixels. The parent uses those pixels
for intrinsic/aspect-ratio layout, typed paint, nearest-neighbor replay,
source-over alpha, clipping, scrolling, hit testing, capture, and image load
events; broken or denied images do not abort the page. Decoded-image caching,
responsive sources, additional CSS image layers beyond the single
background-image URL, SVG image resources, and animated or other image
formats remain active issue #40 browser-completeness work. Exact
evidence is recorded in
[native-engine-browser-211](tasks/native-engine-browser-211.md).

The completed native-engine-browser-212 slice makes external PNG resources
reactive to page-script DOM mutations. After each content-worker script batch,
the native owner retains matching image resources, fetches new or changed
external sources under the same HTTP(S), CSP, mixed-content, referrer, cookie,
and redirect policy, transfers successful RGBA resources in the typed
snapshot, and dispatches the image `load` event before publication. Broken
images remain non-fatal. Decoded HTTP caching, recursive handler loads,
responsive sources, additional CSS image layers beyond the single
background-image URL, SVG image resources, animation, and other formats
remain active issue #40 browser-completeness work. Exact evidence is recorded
in [native-engine-browser-212](tasks/native-engine-browser-212.md).

The completed native-engine-browser-213 slice adds a bounded decoded PNG cache
for external resources. Cacheable responses are retained under requested and
redirected URL keys for both initial discovery and reactive image hydration;
`no-store`, `no-cache`, stale zero-age, privacy-sensitive `Vary`, and
`Set-Cookie` responses are excluded, and policy checks still run before cache
lookup. Duplicate external images therefore reuse one decoded resource without
changing typed layout, paint, capture, hit testing, or load-event ownership.
Image freshness/revalidation is covered by the later 347 slice; stylesheet,
script, Fetch, and Service-Worker CacheStorage freshness, concurrent request coalescing, responsive sources,
additional CSS image layers beyond the single background-image URL, SVG image
resources, animation, and other formats remain active issue #40
browser-completeness work. Exact evidence is recorded in
[native-engine-browser-213](tasks/native-engine-browser-213.md).

The completed native-engine-browser-214 slice adds URL-reflected element
properties across local, content-worker, and same-origin frame projections.
Anchors/areas/bases/links expose resolved `href`, image/script/frame/media
elements expose resolved `src`, and forms expose resolved `action`; assignment
still updates the author attribute through the typed native command bridge.
Relative URL reads use the active document or frame URL, and image `src`
assignment therefore enters the existing native image hydration path. Exact
evidence is recorded in
[native-engine-browser-214](tasks/native-engine-browser-214.md).

The completed native-engine-browser-215 slice adds one native CSS
`background-image` URL layer. Inline PNG data URLs and external PNG resources
now share bounded source identity, HTTP/CSP/mixed-content/referrer/cookie
policy, decoded caching, typed document-wire validation, software paint,
clipping, scrolling, capture, and script-driven `style.backgroundImage`
hydration. It intentionally does not claim CSS repeat/position/size, multiple
layers, gradients, masks, filters, responsive selection, SVG image resources,
animation, or additional formats. Exact evidence is recorded in
[native-engine-browser-215](tasks/native-engine-browser-215.md).

The completed native-engine-browser-216 slice adds bounded
`HTMLImageElement` lifecycle state to the native page surface. Local data-URL
and external PNG images expose `complete`, `naturalWidth`, `naturalHeight`,
and URL-reflected `currentSrc`; attempted external loads, including broken
results, cross the validated content-process wire so the page does not remain
indefinitely pending. Changing or removing `src` clears intrinsic dimensions
and re-enters the pending/empty state, while an image without a source is
complete with zero dimensions. Exact evidence is recorded in
[native-engine-browser-216](tasks/native-engine-browser-216.md).

The completed native-engine-browser-217 slice adds terminal external-image
`error` events. Initial and script-mutated external PNG attempts now produce
either the existing non-bubbling `load` event or a non-bubbling, non-cancelable
`error` event after the content-process policy/loader reaches a result;
failed image attempts remain non-fatal and retain the slice-216 lifecycle
state. Exact evidence is recorded in
[native-engine-browser-217](tasks/native-engine-browser-217.md).

The completed native-engine-browser-218 slice adds image `onload` and
`onerror` handler properties. Assigning a callable handler replaces the prior
handler, assigning a non-callable value removes it, and the properties share
the validated non-bubbling resource-event path with `addEventListener`.
Initial and script-mutated external image witnesses cover replacement and
terminal failure delivery. Exact evidence is recorded in
[native-engine-browser-218](tasks/native-engine-browser-218.md).

The completed native-engine-browser-219 slice adds the standard `img` semantic
role and derives its accessible name from `alt`, including the explicit empty
`alt` case used for decorative images. Existing `aria-label` and
`aria-labelledby` precedence remains intact, and the semantic projection is
shared by local documents and content-process snapshots. Exact evidence is
recorded in [native-engine-browser-219](tasks/native-engine-browser-219.md).

The completed native-engine-browser-220 slice adds bounded responsive image
selection for `srcset` density/width candidates and `sizes` viewport lengths.
The selected candidate drives content-process fetches, intrinsic state, and
`currentSrc`; script mutations to `srcset` or `sizes` invalidate and reload the
selected resource through the existing policy/cache/event path. Exact evidence
is recorded in [native-engine-browser-220](tasks/native-engine-browser-220.md).

The completed native-engine-browser-221 slice adds bounded `<picture>` source
selection. Matching preceding `<source>` elements now participate before the
`<img>` fallback, with simple viewport width media conditions and an explicit
`image/png` type gate; each source reuses the shared `srcset`/`sizes` selector.
Source media mutations reselect and reload the image through the existing
policy/cache/lifecycle/event path, and the typed wire accepts the selected
picture candidate without weakening source identity validation. Exact evidence
is recorded in [native-engine-browser-221](tasks/native-engine-browser-221.md).

The completed native-engine-browser-222 slice adds bounded JPEG image decode
alongside PNG for data URLs and external HTTP(S) resources. Progressive and
baseline JPEGs now produce validated RGBA pixels, intrinsic dimensions,
display-list paint, and `currentSrc`/load state through the same cache, policy,
typed-wire, and capture owners; `<picture type="image/jpeg">` is selectable
when its source wins. Exact evidence is recorded in
[native-engine-browser-222](tasks/native-engine-browser-222.md).

The completed native-engine-browser-223 slice adds bounded static WebP image
decode alongside PNG and JPEG. Local data URLs and external HTTP(S) resources
now produce validated RGBA pixels, intrinsic dimensions, display-list paint,
cache state, and lifecycle/current-source results; `<picture type="image/webp">`
can select the native decoder. Animated WebP remains queued for the frame and
timing owner. Exact evidence is recorded in
[native-engine-browser-223](tasks/native-engine-browser-223.md).

The completed native-engine-browser-224 slice adds bounded static GIF image
decode alongside PNG, JPEG, and WebP. Local data URLs and external HTTP(S)
resources now decode one validated full-canvas frame to RGBA pixels, preserving
intrinsic dimensions, display-list paint, cache state, and lifecycle/current-
source results; `<picture type="image/gif">` can select the native decoder.
Multi-frame, sub-rect, malformed, and over-limit GIFs remain broken until the
native frame, compositing, and repaint owners exist. Exact evidence is
recorded in [native-engine-browser-224](tasks/native-engine-browser-224.md).

The completed native-engine-browser-225 slice adds bounded animated GIF
playback to the shared image pipeline. GIF frames are composited onto the
logical canvas with disposal handling, normalized frame delays, and finite or
infinite loop metadata; local data URLs and HTTP(S) content-process resources
now select the time-appropriate frame during display-list paint, including
CSS background images. Frame count, decoded pixels, transfer payload, and
delay limits remain enforced before publication. Exact evidence is recorded
in [native-engine-browser-225](tasks/native-engine-browser-225.md).

The completed native-engine-browser-226 slice adds bounded animated WebP
playback using the existing pure-Rust decoder. Full logical-canvas frames,
blend/disposal behavior, normalized timing, and loop metadata now flow through
the same local data-URL, HTTP(S) content-process, image paint, and typed-wire
owners as GIF. Frame count, canvas pixels, decoder output, and retained-frame
bytes remain bounded before publication. Exact evidence is recorded in
[native-engine-browser-226](tasks/native-engine-browser-226.md).

The completed native-engine-browser-227 slice adds bounded APNG playback
through the existing PNG decoder. APNG subframes now honor logical-canvas
offsets, source/over blending, none/background/previous disposal, normalized
delays, and finite/infinite loop metadata across local data URLs and HTTP(S)
content-process image resources. `image/apng` is accepted by source selection
and the typed transfer path under the same frame and memory limits. Exact
evidence is recorded in [native-engine-browser-227](tasks/native-engine-browser-227.md).

The completed native-engine-browser-228 slice connects SVG image resources to
the existing native SVG document/layout/raster pipeline. Bounded
`data:image/svg+xml` and HTTP(S) `image/svg+xml` sources now expose intrinsic
dimensions and RGBA display-list pixels through `<img>`, CSS background paint,
and `<picture>` source selection; common `px` viewport dimensions and one-sided
viewBox ratios share the inline SVG sizing path. Decode size, raster surface,
transfer, and recursive data-SVG limits remain enforced before publication.
Exact evidence is recorded in
[native-engine-browser-228](tasks/native-engine-browser-228.md).

The completed native-engine-browser-229 slice adds bounded CSS background
geometry to the existing single background-image layer. Repeat/repeat-x/
repeat-y/no-repeat, keyword/pixel/percentage position, auto/pixel/percentage
size, cover/contain aspect-ratio sizing, edge-tile source crops, and shared
immutable RGBA payloads now flow through local and HTTP(S) content-process
computed style, display-list, raster, capture, and mutation owners. Exact
evidence is recorded in
[native-engine-browser-229](tasks/native-engine-browser-229.md).

The completed native-engine-browser-230 slice adds the common one-layer CSS
`background` shorthand. Supported color, image, repeat, position, and
`/`-separated size components now expand into the existing cascade, source
registry, content-process hydration, display-list, raster, and CSSOM mutation
owners, including the common `center/cover no-repeat` ordering. Exact
evidence is recorded in
[native-engine-browser-230](tasks/native-engine-browser-230.md).

The completed native-engine-browser-231 slice adds bounded CSS relative
positioning. `position: relative` with `top`/`right`/`bottom`/`left` signed
pixel offsets now translates the complete emitted subtree through the shared
layout, paint, scroll, capture, and hit-test geometry without changing sibling
flow allocation; stylesheet, inline, `!important`, CSS-wide reset, and CSSOM
paths share the same computed-style owner. Exact evidence is recorded in
[native-engine-browser-231](tasks/native-engine-browser-231.md).

The completed native-engine-browser-232 slice adds bounded CSS absolute
positioning. `position: absolute` with signed pixel `top`/`right`/`bottom`/`left`
offsets is removed from normal block, flex, and grid allocation and placed
against the nearest positioned, flex, grid, or initial containing block.
Subtree geometry, paint order, overflow, scrolling, capture, and hit testing
continue to use the shared layout output. Exact evidence is recorded in
[native-engine-browser-232](tasks/native-engine-browser-232.md).

The completed native-engine-browser-233 slice adds bounded CSS fixed
positioning. `position: fixed` with signed pixel `top`/`right`/`bottom`/`left`
offsets is anchored to the initial viewport containing block, remains stable
when the root document scrolls, and is isolated from ordinary ancestor
overflow clips. Fixed subtree geometry is rebased through the shared layout,
display-list, raster, capture, and hit-test projection. Exact evidence is
recorded in [native-engine-browser-233](tasks/native-engine-browser-233.md).

The completed native-engine-browser-234 slice adds bounded CSS sticky
positioning. `position: sticky` remains in normal flow while its signed pixel
`top`/`right`/`bottom`/`left` constraints project the complete emitted subtree
against the root scrollport and nearest layout ancestor. Sticky geometry is
rebased through the shared layout, display-list, raster, capture, overflow,
and hit-test projection, including release at the containing-block boundary.
Exact evidence is recorded in
[native-engine-browser-234](tasks/native-engine-browser-234.md).

The completed native-engine-browser-235 slice adds bounded CSS `z-index`
stacking for positioned elements and flex/grid items. Typed cascade values
flow into effective layout stacking levels, stable display-list ordering, and
hit testing, while opacity groups remain atomic at their outer stacking level.
Exact evidence is recorded in
[native-engine-browser-235](tasks/native-engine-browser-235.md).

The completed native-engine-browser-236 slice adds inherited CSS
`pointer-events:auto|none` to the native interaction path. Hit testing skips
non-targetable boxes without changing their paint, while explicit descendant
overrides remain targetable through an inherited `none` ancestor. Exact
evidence is recorded in
[native-engine-browser-236](tasks/native-engine-browser-236.md).

The completed native-engine-browser-237 slice adds bounded nested scrolling
through typed axis-specific `overflow` values. Element `scrollLeft`/
`scrollTop`, `scrollTo`, and `scrollBy` update measured overflow containers;
the resulting offsets feed script geometry, projected overflow clips,
display-list replay, rasterization, capture, and hit testing. Element scroll
events target the scroller without bubbling, root scroll events target the
window realm, and local/content-process page scripts can establish scroll state
during initial navigation. Exact evidence is recorded in
[native-engine-browser-237](tasks/native-engine-browser-237.md).

The completed native-engine-browser-238 slice carries root and nested element
scroll offsets in each history entry. Same-document and full-resource
traversal clamp saved state against the activated document, discard removed
scrollers, and restore valid nested positions; HTTP(S) traversal synchronizes
the restored pair into the content process before lifecycle events. Exact
evidence is recorded in
[native-engine-browser-238](tasks/native-engine-browser-238.md).

The completed native-engine-browser-239 slice composes live child-frame native
surfaces into the visible parent capture, recursively through nested frames.
Async backend/runtime, CLI, and MCP capture paths discover frames before
rasterization, and the public capability profile now reflects the completed
nested-scroll and IndexedDB owners. Exact evidence is recorded in
[native-engine-browser-239](tasks/native-engine-browser-239.md).

The completed native-engine-browser-240 slice routes native point clicks into
the deepest visible nested frame, translating coordinates through each frame
owner and preserving child event/effect ownership. Capture and input now share
the same clipped source offset, so parent scrolling keeps pixels and pointer
coordinates aligned. Exact evidence is recorded in
[native-engine-browser-240](tasks/native-engine-browser-240.md).

The completed native-engine-browser-241 slice preserves focused input across
native frame boundaries. Successful nested point clicks establish a target-
local focused frame, later key actions route to that child engine, and frame
rebuilds clear stale focus identities. Exact evidence is recorded in
[native-engine-browser-241](tasks/native-engine-browser-241.md).

The completed native-engine-browser-242 slice negotiates each embedded native
frame’s viewport from its parent owner content box. Child layout, raster,
capture, hit-testing, and local actions now share the dimensions of the
visible frame surface, including nested frame levels. Exact evidence is
recorded in [native-engine-browser-242](tasks/native-engine-browser-242.md).

The completed native-engine-browser-243 slice adds sequential keyboard focus
traversal inside the focused native frame. `Tab` and `Shift+Tab` share a
bounded positive-`tabindex`/document-order sequence, preserve preventDefault
and blur/focus event effects, and are covered in both local and HTTP
content-process paths. Exact evidence is recorded in
[native-engine-browser-243](tasks/native-engine-browser-243.md).

The completed native-engine-browser-244 slice routes semantic targeted
actions through the selected frame subtree. `Click`, `Type`, `Clear`,
`Check`, `Uncheck`, and `Select` can reach a unique child-frame locator while
preserving the existing event/effect owner and rejecting cross-frame
ambiguity. Exact evidence is recorded in
[native-engine-browser-244](tasks/native-engine-browser-244.md).

The completed native-engine-browser-245 slice carries frame ownership through
semantic inspection, intent resolution, preflight, and execution. Child
targets now expose an optional `frameId`, the observation revision covers the
attached frame tree, unique preflight records the owning frame, and semantic
actions plus popup clicks dispatch through that exact route. Exact evidence is
recorded in [native-engine-browser-245](tasks/native-engine-browser-245.md).

The completed native-engine-browser-246 slice wires the existing pointer
commands into the native action owner. Local and HTTP(S) documents now execute
double-click, hover, and source-to-destination drag with bounded DOM event
ordering; child-frame locators resolve both drag endpoints within one frame;
and CLI/MCP dispatches use the shared semantic action contract. Exact evidence
is recorded in
[native-engine-browser-246](tasks/native-engine-browser-246.md).

The completed native-engine-browser-247 slice wires bounded file selection into
the native action owner. Local and HTTP(S) documents now attach validated
in-memory file objects to unique `input[type=file]` controls, expose
`FileList`/`File` metadata and bytes to the page realm, preserve the browser
fake path and `FormData` file projection, and dispatch `input`/`change` through
the existing event bridge. CLI and MCP upload paths enforce the upload policy
before copying regular files into the native boundary. Exact evidence is
recorded in [native-engine-browser-247](tasks/native-engine-browser-247.md).

The completed native-engine-browser-248 slice carries bounded binary request
bodies through the shared native transport. Selected files now reach both
`fetch(FormData)` and multipart form navigation in HTTP(S) content workers
with exact raw bytes, filenames, media types, and boundaries; text/plain and
URL-encoded submissions retain their existing behavior. `ArrayBuffer` and
`ArrayBufferView` fetch bodies use the same bounded byte path, while content
worker IPC rejects ambiguous or oversized body representations. Exact evidence
is recorded in [native-engine-browser-248](tasks/native-engine-browser-248.md).

The completed native-engine-browser-196 slice closes the bounded attribute-node
Web IDL surface. `document.createAttribute()` now returns persistent `Attr`
objects with value/node-value accessors and ownership; element attribute-node
methods preserve replacement/removal identity and typed errors; and
`element.attributes` exposes a live indexed/iterable `NamedNodeMap`. Local,
HTTP(S) content-worker, and same-origin-frame paths share the surface through
the native host. At that checkpoint namespace-qualified attributes, complete
Web IDL descriptors, XML documents, and browser-wide conformance remained issue
#40 work; the namespace attribute gap is closed by slice 201. Exact evidence is
recorded in
[native-engine-browser-196](tasks/native-engine-browser-196.md).

The completed native-engine-browser-195 slice advances the HTML tree builder's
recovery behavior. Unterminated comments, bogus declarations, and
EOF-terminated tags now recover to bounded Comment/Text nodes; duplicate HTML
attributes keep their first value; late or duplicate doctypes are ignored; and
common paragraph, list, option, ruby, and table implied-end-tag cases close in
the native owner. Detached `innerHTML` parsing mirrors the same recovery and
auto-close rules, with local persistent-realm coverage. The contract and exact
evidence are recorded in
[native-engine-browser-195](tasks/native-engine-browser-195.md); full WHATWG
tree construction, foreign content, and Web IDL/conformance promotion remain
issue #40 work.

The completed native-engine-browser-194 slice closes document-type construction
after parsed doctype projection. `document.implementation.createDocumentType()`
now creates bounded `DocumentType` nodes that can be cloned and inserted at the
document root with duplicate and hierarchy validation; local, HTTP(S)
content-worker, same-origin frame, and nested-frame paths preserve metadata and
identity across refresh. The contract and exact evidence are recorded in
[native-engine-browser-194](tasks/native-engine-browser-194.md); broader
DOMImplementation factories, complete document tree-builder constraints, and
Web IDL/conformance promotion remain issue #40 work. The malformed-markup
recovery subset listed here was subsequently closed by
[native-engine-browser-195](tasks/native-engine-browser-195.md).

The completed native-engine-browser-193 slice closes the script-created comment
gap. `document.createComment()` now returns a bounded `Comment` node that can
be inserted, mutated, cloned, serialized, and retained across host projection
refreshes in local, HTTP(S) content-worker, and same-origin frame realms.
Detached `innerHTML` parsing also retains comments, while comment data remains
outside visible text, layout, and paint. The typed command and frame-batch
routes are covered by local, content-worker, same-origin-frame, and
nested-frame witnesses. The contract and exact evidence are recorded in
[native-engine-browser-193](tasks/native-engine-browser-193.md); document-type
construction, full malformed-comment recovery, and broader Web IDL/conformance
promotion remain issue #40 work. The bounded malformed-comment recovery subset
was subsequently closed by
[native-engine-browser-195](tasks/native-engine-browser-195.md).

The completed native-engine-browser-192 slice preserves parsed HTML comments
and basic document-type metadata as real native DOM nodes across local,
HTTP(S) content-worker, same-origin frame, and nested-frame projections.
Comments expose the supported CharacterData mutation path while remaining out
of visible text, layout, and paint; doctypes expose `document.doctype`, type,
name, and parentage. The typed wire and persistent script snapshot carry the
new node kinds, and document serialization preserves the supported comment and
doctype forms. The bounded contract and exact evidence are recorded in
[native-engine-browser-192](tasks/native-engine-browser-192.md); complete
malformed-HTML recovery, raw-text/foreign-content parsing, script-created
comment/doctype construction, and Web IDL conformance remain issue #40
promotion work.

The completed native-engine-browser-191 slice preserves script-created element
and text-node identity across separate evaluations in local, HTTP(S)
content-worker, and same-origin frame realms. The typed native snapshot now
carries generation-scoped temporary-node identities, and the persistent
JavaScript realms reuse the original wrappers when committed native nodes are
projected again. Queries and mutations in the next evaluation therefore retain
object identity and native ownership; transaction failure does not publish a
partial mapping. The contract and evidence are recorded in
[native-engine-browser-191](tasks/native-engine-browser-191.md).

The completed native-engine-browser-190 slice extends structural DOM behavior
across local, HTTP(S) content-worker, and same-origin frame realms. Native
nodes now expose `cloneNode()`, `isSameNode()`, `isEqualNode()`,
`compareDocumentPosition()`, and `normalize()` over the shared tree projection.
Shallow and deep element/text/fragment clones preserve attributes and child
order, disconnected and ancestor/descendant document-position flags are
deterministic, and adjacent/empty text normalization commits through the
existing typed mutation transaction, including children of a detached node
created in the same script batch. The contract and evidence are recorded in
[native-engine-browser-190](tasks/native-engine-browser-190.md).

The completed native-engine-browser-189 slice closes the structural DOM
identity/replacement gap across local, HTTP(S) content-worker, and same-origin
frame realms. Native nodes now expose a root-preserving `getRootNode()` and
attached elements expose live `outerHTML` reads plus bounded parsed replacement
writes through the existing tree ownership and command queue. Temporary node
identities remain unique across host bootstrap refreshes, and replacement
state is visible to selectors, collections, parent links, and serialization in
the same script turn. The contract and evidence are recorded in
[native-engine-browser-189](tasks/native-engine-browser-189.md).

The completed native-engine-browser-188 slice makes document titles live across
the native document owners. `document.title` now reads the current `<title>`
node, writes through the existing text mutation transaction, and materializes a
missing title/head or uses a bounded Rust-owned fallback when no HTML root is
present. Local, HTTP(S) content-worker, and same-origin frame realms share the
same behavior, and title changes survive the next host refresh. The contract
and local evidence are recorded in
[native-engine-browser-188](tasks/native-engine-browser-188.md).

The completed native-engine-browser-187 slice exposes live document-facing DOM
surfaces across local, HTTP(S) content-worker, and same-origin frame realms.
Documents now provide head, forms, links, scripts, images, scrollingElement,
live tag/class collections, and getElementsByName while reusing the existing
tree ownership and collection identity machinery. The contract and local
evidence are recorded in
[native-engine-browser-187](tasks/native-engine-browser-187.md).

The completed native-engine-browser-186 slice exposes the shared character-data
contract across local, HTTP(S) content-worker, and same-origin frame realms.
Text nodes now use the `Text` → `CharacterData` → `Node` identity chain,
synchronize `nodeValue`, `data`, and `textContent`, and implement bounded
`length`, `substringData`, `appendData`, `insertData`, `deleteData`, and
`replaceData` mutations through the existing native command/observer path. The
contract and local evidence are recorded in
[native-engine-browser-186](tasks/native-engine-browser-186.md).

The completed native-engine-browser-185 slice hardens bounded HTML fragment
parsing with a quote-aware tag scanner, declaration/comment skipping, and
raw-text/RCDATA handling for `script`, `style`, `textarea`, and `title`.
Local, HTTP(S) content-worker, and same-origin frame fragment construction
now preserves `>` inside quoted attributes, keeps script/style source literal,
decodes textarea/title entities, and avoids synthetic declaration/comment
nodes. The contract and local evidence are recorded in
[native-engine-browser-185](tasks/native-engine-browser-185.md).

The completed native-engine-browser-184 slice materializes element
`textContent` and `innerText` mutations in the current script turn. Local,
HTTP(S) content-worker, and same-origin frame elements now expose one live
text child immediately, with matching `childNodes`, parent links,
serialization, and `MutationObserver` added-node payloads; the authoritative
Rust command remains one `setTextContent` transaction while preview commands
are suppressed. The contract and local evidence are recorded in
[native-engine-browser-184](tasks/native-engine-browser-184.md).

The completed native-engine-browser-183 slice makes element `innerHTML`
mutation visible in the current script turn. Local, HTTP(S) content-worker,
and same-origin frame elements now parse bounded nested markup into live host
children for immediate selectors, text, serialization, and observer records;
the authoritative Rust command remains one `setInnerHtml` transaction while
temporary parser commands are suppressed. Full HTML parsing, Web IDL
reflection, and browser-wide promotion remain issue #40 gates. Its contract
and local evidence are recorded in
[native-engine-browser-183](tasks/native-engine-browser-183.md).

The completed native-engine-browser-182 slice extends the attribute-backed
DOM surface to common form and HTML properties: boolean properties such as
`disabled`, `hidden`, `multiple`, `required`, and `readOnly`, plus common
string properties and normalized `type`, now round-trip through native
attribute commands in local, HTTP(S) content-worker, and same-origin frame
realms. Existing actionability state remains refresh-safe, and property writes
do not enqueue commands during snapshot hydration. Full Web IDL reflection and
browser-wide promotion remain issue #40 gates. Its contract and local evidence
are recorded in [native-engine-browser-182](tasks/native-engine-browser-182.md).

The completed native-engine-browser-181 slice adds reflected `id` and
`className` accessors to local, HTTP(S) content-worker, and same-origin frame
elements. Property assignment now uses the native attribute transaction, while
attribute methods continue to update the same reflected view without recursive
commands. Document `getElementById`, selector, tag-name, class-name, and active
element queries now walk the live attached tree, so newly appended nodes are
visible in the same script turn. Full Web IDL reflection and browser-wide
promotion remain issue #40 gates. Its contract and local evidence are recorded
in [native-engine-browser-181](tasks/native-engine-browser-181.md).

The completed native-engine-browser-180 slice extends `MutationObserver` to
document fragments. Fragment targets now accept child-list observation with
ordered added/removed nodes and sibling payloads for detached insertion,
reparenting, removal, and fragment staging; moving an attached node through a
fragment also emits the native removal command needed to keep Rust state
authoritative. Local, HTTP(S) content-worker, and same-origin frame realms
share the contract, while broader observer/Web IDL parity and browser-wide
promotion remain issue #40 gates. Its contract and local evidence are recorded
in [native-engine-browser-180](tasks/native-engine-browser-180.md).

The completed native-engine-browser-179 slice adds bounded fragment
`innerHTML` parsing and live same-turn tree queries. Detached fragments now
construct nested elements/text, decode common and numeric entities, preserve
attributes and void elements, and expose selector/collection traversal before
attachment; local, HTTP(S) content-worker, and same-origin frame realms share
the behavior. Newly attached nodes are also visible through element query
methods during the same script turn. The parser remains a bounded tree builder
rather than a claim of full HTML parsing; malformed-input, raw-text/
foreign-content rules and complete Web IDL descriptors remain open. Its
contract and local evidence are recorded in
[native-engine-browser-179](tasks/native-engine-browser-179.md); cross-document
session history, bfcache, cross-origin history, and browser-wide parity remain
issue #40 promotion gates.

The completed bounded Response-constructor slice is
[native-engine-browser-120](tasks/native-engine-browser-120.md). Fetched and
constructed responses now share `Response` identity; `new Response`,
`Response.json`, `Response.error`, and `Response.redirect` reuse the bounded
body/header/clone projections, with null bodies exposing `body === null`.
Stream input, body disturbance/`bodyUsed`, complete factory and redirect/error
internals, trailers, shared tee/backpressure, and complete Response Web IDL
identity remain open.

The completed bounded response-Headers-identity slice is
[native-engine-browser-121](tasks/native-engine-browser-121.md). Fetch and
asynchronous XHR response-header views now satisfy `headers instanceof Headers`
while remaining immutable normalized snapshots; lookup, duplicate combination,
iteration, filtering, and `forEach()` remain unchanged, and response mutation
methods reject. Raw header bytes, trailers, live mutation, descriptor parity,
and complete Headers/Fetch/XHR Web IDL parity remain open.

Full cookie policy/Web IDL parity and the remaining browser-complete gates
remain open.

The first dependency-ordered checkpoint is
[native-engine-001](tasks/native-engine-001.md): a default-off,
fixture/data-URL-only, one-context engine kernel and explicit semantic backend.
It does not claim browser parity or remote-content safety.

The completed dependency-ordered slices are
[native-engine-002](tasks/native-engine-002.md), which adds bounded semantic
DOM projection and revision-bound locators;
[native-engine-003](tasks/native-engine-003.md), which adds the first
revisioned click/type/focus mutation path and effects signal;
[native-engine-004](tasks/native-engine-004.md), which adds deterministic
single-select/option state;
[native-engine-005](tasks/native-engine-005.md), which adds a bounded
visibility/actionability gate; and
[native-engine-006](tasks/native-engine-006.md), which hardens raw-text and
RCDATA handling. They remain semantic-only: CSS/layout hit testing,
JavaScript, network, and raw form-value evidence are not claimed.
[native-engine-007](tasks/native-engine-007.md), which adds a narrow
CSS-presentation model for selector-driven `display`/`visibility` state.
General CSS, scrolling/stacking layout, and screenshot/capture paint remain
unimplemented; later native slices add only bounded display-list and
software-surface artifacts.

The completed runtime integration slice is
[native-engine-008](tasks/native-engine-008.md). It adds a feature-gated
`BrowserRuntime::Native`, an explicit Rust session constructor, and a local
one-shot CLI path for navigate/click/type/text/observe/targets. The CLI default
configuration at that boundary accepted only `about:blank` and bounded
percent-decoded `data:text/html`; it did not register fixtures or contact
endpoints. The later 036 slice below adds bounded standard padded-base64
navigation. Unsupported flags, remote URLs, script/evaluate, MCP, and TUI
remain fail-closed.

The completed layout/input slice is
[native-engine-009](tasks/native-engine-009.md). It owns bounded integer-pixel
normal-flow geometry, Rust-only layout inspection, deterministic point
hit-testing, and the native `point=x,y` click-target extension. It does not
add screenshots, capture, scrolling, general CSS, or geometry to the stable
transport evidence contract.

The completed display-list slice is
[native-engine-010](tasks/native-engine-010.md). It adds a deterministic,
Rust-only clear/fill/text display list from the current layout revision and a
bounded solid-color CSS subset. It does not add screenshots, fonts, images, or
a paint capability to the stable backend contract.

The completed software-surface slice is
[native-engine-011](tasks/native-engine-011.md). It consumes that list into a
bounded logical RGBA surface with a small built-in glyph subset. It does not
add PNG/screenshots, font loading, GPU/window APIs, or a capture capability to
the stable backend contract. The next renderer slice must be documented and
committed separately before it expands this boundary.

The completed style-inheritance slice is
[native-engine-012](tasks/native-engine-012.md). It resolves inherited text
color through the bounded DOM chain and feeds the existing display-list and
software-surface artifacts. It does not add general CSS, inherited layout,
fonts, images, screenshot/capture transport, or stable backend capabilities.
The next renderer slice must be documented and committed separately before it
expands this boundary.

The completed paint-clipping slice is
[native-engine-013](tasks/native-engine-013.md). It adds bounded
`overflow:hidden` clip rectangles to fill/text display commands and enforces
them during Rust-only surface replay. It does not add nested scrolling, stacking,
borders, transforms, screenshots, or capture transport. The next renderer slice
must be documented and committed separately before it expands this boundary.

The completed uniform-border slice is
[native-engine-014](tasks/native-engine-014.md). It adds a bounded
`border:Npx solid <color>` CSS declaration, a revisioned `BorderRect` display
command, and inside-the-box software replay using the existing clip and
source-over rules. It does not add border box-model geometry, padding,
box-sizing, individual sides, non-solid styles, scrolling, transforms, or
screenshot evidence. Capture transport is separately owned by
[native-engine-015](tasks/native-engine-015.md).

The completed capture slice is
[native-engine-015](tasks/native-engine-015.md). It adds bounded PNG encoding
for the existing logical RGBA surface and exposes explicit
`CaptureFormat::Png` through the native backend while keeping screenshot-
containing evidence, JPEG/PDF, physical pixels, and native CLI screenshots
unsupported. It was verified and committed before later renderer and resource
expansions changed this boundary.

The completed box-model slice is
[native-engine-016](tasks/native-engine-016.md). It adds bounded uniform
padding and margin, explicit content-box/border-box sizing, outer/content
layout rectangles, and content-origin child/text placement. General box-model
and layout behavior remains explicitly outside the native capability claim.

The completed viewport-scroll slice is
[native-engine-017](tasks/native-engine-017.md) adds bounded vertical root
scrolling across layout hit testing,
display-list replay, capture, and revisioned action effects; horizontal,
nested, smooth, and keyboard scrolling remain outside the current claim.

The completed side-specific-border slice is
[native-engine-018](tasks/native-engine-018.md). It adds independently
cascaded physical solid borders, side-aware box-model insets, and deterministic
clipped/scrolled display replay; other border styles, radii, logical
writing-mode sides, and browser corner-join fidelity remain outside the
current claim.

The completed bounded-pattern-border slice is
[native-engine-019](tasks/native-engine-019.md). It implements
typed `solid`/`dashed`/`dotted` physical border styles with deterministic
integer patterns; other border styles, radius/images/gradients, standalone
style properties, logical sides, and browser dash/corner fidelity remain
outside the current claim.

The completed bounded-corner-radius slice is
[native-engine-020](tasks/native-engine-020.md). It adds bounded physical
one-to-four-value `border-radius` shorthand expansion, conservative corner
normalization, rounded fill/border replay, and rounded point hit testing.
Percentages, elliptical radii, corner longhands, rounded descendant clips,
anti-aliasing, and browser corner fidelity remain outside the current claim.

The completed bounded-inline-flow slice is
[native-engine-021](tasks/native-engine-021.md). It adds preflight inline-box
line placement using the same bounded integer outer-width calculation as final
layout, preserving deterministic line height and hit/paint coordinates while
leaving typography and general inline formatting unsupported.

The completed bounded line-height slice is
[native-engine-022](tasks/native-engine-022.md). It adds a positive fixed pixel
`line-height` property as a local flow minimum while preserving explicit height
precedence and leaving font metrics, general inheritance, and browser
line-layout behavior unsupported.

The completed bounded direct-text-flow slice is
[native-engine-023](tasks/native-engine-023.md). It carries collapsed direct
text fragments from the shared flow cursor into source-ordered display paint,
repairing mixed text/inline origins while leaving typography and CSS whitespace
behavior unsupported.

The completed bounded word-wrap slice is
[native-engine-024](tasks/native-engine-024.md). It keeps collapsed words
together when the fixed line fits and splits only over-wide words, while
retaining source-ordered fragments and the existing typography limitations.

The completed physical box-edges slice is
[native-engine-025](tasks/native-engine-025.md). It expands bounded one-to-four
value physical `padding`/`margin` shorthands, supports their top/right/bottom/
left longhands with independent cascade, and feeds side-aware values through
content origins and normal-flow margins. Logical sides, invalid/negative/
percentage/`auto` values, margin collapsing, and general CSS layout remain
outside the native capability claim.

The completed whitespace-boundaries slice is
[native-engine-026](tasks/native-engine-026.md). It preserves bounded source
whitespace boundaries across sibling direct text, `display:contents`, and
supported inline flow items, paints consumed separators through the existing
text-fragment path, and drops separators at wrapped line starts. CSS
`white-space` modes, typography, and cross-owner inline parity remain outside
the native capability claim.

The completed overflow hit-test/projection slice is
[native-engine-027](tasks/native-engine-027.md). It shares the bounded
rectangular `overflow:hidden` ancestor intersection across software paint,
viewport rectangle projection, and point hit-testing, including nested clips in
document coordinates before root-scroll translation. Visible overflow,
axis-specific/nested scrolling, rounded descendant clips, and general CSS
hit-testing remain outside the native capability claim.

The completed unsupported-CSS-diagnostics slice is
[native-engine-028](tasks/native-engine-028.md). It makes ignored selectors,
properties, values, and malformed CSS observable through a bounded revisioned
Rust API without changing stable backend evidence or echoing raw stylesheet
content. Existing deterministic CSS omission/fallback behavior remains intact.

The completed pixel-golden-capture slice is
[native-engine-029](tasks/native-engine-029.md). It certifies the existing
bounded logical surface and decoded PNG bytes against one complete fixed
fixture golden without changing screenshot evidence or renderer scope. Its
focused golden, native integration, native unit, strict lint, full locked
all-target/all-feature, doctest, formatting, and documentation gates are
recorded in the task file and issue #40.

The completed descendant-selector slice is
[native-engine-030](tasks/native-engine-030.md). It extends the bounded CSS
grammar with ancestor-scoped compound selectors while preserving explicit
limits, diagnostics, cascade precedence, and the no-general-CSS boundary. Its
implementation and full validation evidence are recorded in the task file and
issue #40.

The completed overflow-clip slice is
[native-engine-031](tasks/native-engine-031.md). It accepts non-scrolling
`overflow: clip` through the existing bounded rectangular clip path while
preserving root-scroll, hit-testing, diagnostics, and no-general-CSS limits.
Its implementation and full validation evidence are recorded in the task file
and issue #40.

The completed hard-line-break slice is
[native-engine-032](tasks/native-engine-032.md). It treats visible `<br>`
elements as bounded hard line breaks in the existing inline-flow cursor while
preserving hidden-state handling, text ownership, fixed line-height, and the
no-general-CSS boundary. Its implementation and full validation evidence are
recorded in the task file and issue #40.

The completed pre-line-break slice is
[native-engine-033](tasks/native-engine-033.md). It supports inherited
`white-space: pre-line` source newline breaks through the same bounded flow
cursor while retaining whitespace collapsing, CRLF normalization, and the
no-general-CSS boundary. Its implementation and full validation evidence are
recorded in the task file and issue #40.

The completed preformatted-whitespace slice is
[native-engine-034](tasks/native-engine-034.md). It adds inherited
`white-space: pre` with literal fixed-cell source whitespace, LF/CR/CRLF hard
breaks, and no soft wrapping, while retaining explicit limits for wide-line
overflow, tab stops, font metrics, text alignment, and general CSS whitespace
conformance.

The completed pre-wrap-whitespace slice is
[native-engine-035](tasks/native-engine-035.md). It extends that inherited
source-whitespace path with fixed-cell soft wrapping for `white-space: pre-wrap`,
while retaining explicit limits for browser line breaking, tab
stops, font metrics, shaping, bidi, and general CSS conformance.

The completed base64-data-url slice is
[native-engine-036](tasks/native-engine-036.md). It adds bounded standard
base64 `data:text/html` loading through the existing native navigation and
dispatcher path, while retaining explicit local-only, UTF-8, size, and
non-network limits. Its implementation and full validation evidence are
recorded in the task file and issue #40.

The completed local-navigation-history slice is
[native-engine-037](tasks/native-engine-037.md). It adds bounded raw-fragment
same-document navigation and explicit Rust back/forward traversal for local
resources, while retaining parse-before-commit, revision, history, scroll, and
non-network limits. Its implementation and validation evidence are recorded in
the task file and issue #40.

The completed local-link-activation slice is
[native-engine-038](tasks/native-engine-038.md). It wires semantic local anchor
clicks through the existing action and navigation owner, while retaining
fragment, parse-before-commit, revision, history, scroll, and non-network
limits. Its implementation and validation evidence are recorded in the task
file and issue #40.

The completed fragment-target-scroll slice is
[native-engine-039](tasks/native-engine-039.md). It adds exact visible local
fragment-target scrolling and saved root-scroll restoration across bounded
history traversal, while retaining raw-fragment, parse-before-commit, and
non-network limits. Its implementation and validation evidence are recorded in
the task file and issue #40.

The completed fixture-relative-link slice is
[native-engine-040](tasks/native-engine-040.md). It adds bounded same-host
relative resolution for registered fixtures while retaining explicit local
resource loading, parse-before-commit, fragment scrolling, and failure-atomic
navigation limits.

The completed percent-decoded-fragment-target slice is
[native-engine-041](tasks/native-engine-041.md). It decodes bounded UTF-8
percent escapes before exact visible local `id` matching while retaining the
existing duplicate-safe root scrolling, history restoration, local-only
resource, and malformed-target failure behavior.

The completed legacy-name-fragment-target slice is
[native-engine-042](tasks/native-engine-042.md). It adds a bounded exact
legacy `<a name>` fallback after decoded `id` lookup while retaining ID
precedence, duplicate-safe scrolling, and the existing local navigation and
history boundaries.

The completed bounded-text-fragment-target slice is
[native-engine-043](tasks/native-engine-043.md). It adds a bounded
`#:~:text=start[,end]` match against the first visible non-truncated text run
with per-term UTF-8 decoding while retaining the existing fragment, scroll,
history, and local-resource limits. The completed follow-on
[native-engine-044](tasks/native-engine-044.md) adds bounded exact prefix and
suffix affixes around that one-run matcher while retaining the current raw
comma grammar, per-term decoding, scroll, history, and fail-closed boundaries.
The completed follow-on [native-engine-045](tasks/native-engine-045.md) adds
bounded root horizontal scrolling from measured overflow width while retaining
independent clamping and the existing viewport, hit-test, display-list, raster,
and history contracts. The completed follow-on
[native-engine-046](tasks/native-engine-046.md) adds bounded inherited
`white-space: nowrap` through the same fixed-cell flow and measured root
horizontal scroll path. Its implementation and validation evidence are
recorded in the task file and issue #40.

The completed dependency-ordered slice
[native-engine-047](tasks/native-engine-047.md) propagated the existing
positive pixel `line-height` floor through the DOM style walk while preserving
explicit child declarations and height precedence. The completed follow-on
[native-engine-048](tasks/native-engine-048.md) makes measured root horizontal
overflow consume the existing `overflow:hidden`/`overflow:clip` intersection
so fully clipped text cannot create a false scroll range.

The completed dependency-ordered slice
[native-engine-049](tasks/native-engine-049.md) adds independently cascaded
`overflow-x:hidden`/`clip` and `overflow-y:hidden`/`clip` rectangles through
the existing paint, viewport, hit-test, and root-overflow owners.

The completed dependency-ordered slice
[native-engine-050](tasks/native-engine-050.md) adds bounded physical
`min-width`/`max-width`/`min-height`/`max-height` constraints through the
existing content-box and border-box geometry owner.

The completed dependency-ordered slice
[native-engine-051](tasks/native-engine-051.md) adds bounded CSS opacity groups
through the existing display-list and software-rasterizer owners. Subtree
compositing is explicit and bounded; layout and hit-testing do not treat
opacity as visibility. The completed follow-on dependency-ordered slice
[native-engine-052](tasks/native-engine-052.md) adds bounded inherited
`text-align` for fixed-cell direct text and supported inline flow. The completed
follow-on dependency-ordered slice
[native-engine-053](tasks/native-engine-053.md) extends the existing bounded
color grammar with fixed-point `rgba(R, G, B, A)` alpha for background, border,
and text paint while preserving the current display-list and raster owners.
Its implementation and validation evidence are recorded in the task file and
issue #40. No general CSS Color 4 or color-management parity is implied. The
completed follow-on dependency-ordered slice
[native-engine-054](tasks/native-engine-054.md) adds inherited fixed-cell
`text-decoration: none|underline` to text display commands and software
replay without changing layout or hit-testing ownership. Its implementation
and validation evidence are recorded in the task file and issue #40. The
completed follow-on [native-engine-055](tasks/native-engine-055.md) task adds inherited
ASCII `text-transform: none|uppercase|lowercase` during fixed-cell layout so
wrapping, text-fragment matching, display-list projection, and root-overflow
measurement share one transformed output; semantic source text remains
unchanged and Unicode/locale/font parity remains outside the boundary. Its
implementation and validation evidence are recorded in the task file and issue
#40. The completed follow-on [native-engine-056](tasks/native-engine-056.md)
task adds non-negative fixed-pixel `text-indent` to the first line of block
containers, clamps it to retain one fixed cell, and leaves inline and
`display:contents` elements on their containing block's flow. Its local
implementation and validation evidence are recorded in the task file and
issue #40; remote CI remains pending until this branch is pushed.
The completed follow-on [native-engine-057](tasks/native-engine-057.md) task
adds bounded inherited non-negative fixed-pixel `word-spacing` across the
existing collapsed and supported preformatted ASCII-space flow. Its measured
advance is shared by wrapping, text fragments, alignment, display-list
projection, raster replay, hit testing, and root-overflow measurement; the
implementation and validation evidence are recorded in the task file and
issue #40. Remote CI remains pending until this branch is pushed.
The completed follow-on [native-engine-058](tasks/native-engine-058.md) task
defines bounded inherited non-negative fixed-pixel `letter-spacing` after every
rendered fixed-cell character in each emitted fragment, composed with
`word-spacing` on ASCII spaces. Its measured advance is shared by wrapping,
preformatted chunking, text fragments, alignment, display-list projection,
raster replay, hit testing, and root-overflow measurement; browser
pair-boundary, Unicode, font-metric, negative, relative, percentage, and
`normal` semantics remain outside the boundary. Implementation and local
validation evidence are recorded in the task file and issue #40; remote CI
remains pending until this branch is pushed.
The completed follow-on [native-engine-059](tasks/native-engine-059.md) task
adds inherited `font-weight: normal|bold|400|700` to the fixed-cell text
presentation path. `normal`/`400` retain the current glyph replay and
`bold`/`700` add a clipped one-pixel horizontal dilation without changing
advances, layout, semantics, hit testing, overflow, or text-fragment
coordinates. Font selection, metrics, shaping, variable weights, and browser
text-rendering parity remain outside the boundary. Implementation and local
validation evidence are recorded in the task file and issue #40; remote CI
remains pending until this branch is pushed.
The completed follow-on [native-engine-060](tasks/native-engine-060.md) task
adds inherited `font-style: normal|italic` to the fixed-cell text presentation
path. `normal` retains the current glyph replay and `italic` applies a
deterministic clipped row-dependent horizontal shear without changing
advances, layout, semantics, hit testing, overflow, or text-fragment
coordinates. Bold dilation, underline, spacing, opacity, scrolling, and
capture compose through the existing immutable text-command and raster owners.
Oblique forms, angles, font selection/loading/metrics, shaping, anti-aliasing,
and browser text-rendering parity remain outside the boundary. Implementation
and local validation evidence are recorded in the task file and issue #40;
remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-061](tasks/native-engine-061.md) slice
adds inherited `word-break: normal|break-all` to the bounded collapsed
fixed-cell flow path. `normal` retains word-aware wrapping; `break-all` allows
deterministic character-boundary splitting for every collapsed word while
preserving the existing separator, spacing, fragment, overflow, and semantic
owners. `pre`, `pre-wrap`, and `nowrap` retain their established behavior.
Unicode line-breaking, grapheme policy, hyphenation, `overflow-wrap`, bidi,
writing modes, font metrics, and browser conformance remain outside the
boundary. The design is `14d7fc4`, the implementation is `479f3a3`, and the
documentation closeout is `433d6fd`; local validation evidence is recorded in
the task file and remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-062](tasks/native-engine-062.md) slice
adds local `text-overflow: clip|ellipsis` to the bounded single-line
fixed-cell path. `clip` retains the existing full visual run under a horizontal
overflow clip; eligible `ellipsis` blocks replace an overflowing suffix with a
spacing-aware fixed-cell ASCII `...` marker while preserving the full semantic
source text. The implementation is restricted to one direct text child in a
rendered `nowrap` block with finite horizontal clipping; multi-line
truncation, nested inline formatting, Unicode ellipsis behavior, and browser
conformance remain outside the boundary. The design is `7e488aa`, the
implementation is `e4c5bb1`, and local validation evidence is recorded in the
task file; remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-063](tasks/native-engine-063.md) slice
adds inherited `vertical-align: baseline|top|middle|bottom` to the existing
fixed-cell inline-flow line-item owner. `baseline` preserves the current
top-origin behavior; `top`, `middle`, and `bottom` apply bounded integer
offsets within the existing line box and move an inline item's boxes and text
artifacts together. Font metrics, typographic baselines, lengths, percentages,
bidi, writing modes, ruby, table-cell alignment, and browser conformance remain
outside the boundary. The design is `7721df2` and the implementation is
`facd2f6`; local validation evidence is recorded in the task file and remote
CI remains pending until this branch is pushed.

The completed follow-on [native-engine-064](tasks/native-engine-064.md) slice
adds bounded block-level `display: flex` single-row placement for eligible
direct element children. Items retain source order, fixed explicit/intrinsic
widths, and margins without grow, shrink, wrap, reverse, gap, or cross-axis
distribution. Containers with meaningful direct text, `display: contents`, or
visible `<br>` children use the existing normal-flow fallback so content is not
dropped. The design is `a05bdd6`, the implementation is `7c38354`, and local
validation evidence is recorded in the task file. Remote CI remains pending
until this branch is pushed.

The completed follow-on [native-engine-065](tasks/native-engine-065.md) slice
adds one non-negative fixed-pixel `gap` between visible direct element items in
an eligible bounded flex row. Hidden and `display:none` items do not consume a
gap position; ineligible containers retain normal-flow fallback. Multi-value
and percentage gap grammar, `row-gap`, `column-gap`, flex distribution,
wrapping, and general Flexbox remain outside the boundary. The contract is
recorded in the task file. The design is `062998c`, the implementation is
`28735c4`, and local validation evidence is recorded there. Remote CI remains
pending until this branch is pushed.

The completed follow-on [native-engine-066](tasks/native-engine-066.md) slice
adds bounded `justify-content:flex-start|center|flex-end|space-between` to
eligible fixed-width flex rows. Positive free space is placed before the row
or distributed across its existing gaps with deterministic integer rounding;
overflow is never moved to a negative coordinate. Flex growth/shrink, wrapping,
direction, cross-axis alignment, `space-around`, `space-evenly`, and general
Flexbox remain outside the boundary. The design is `a53b10f`, the implementation
is `3fe5306`, and local validation evidence is recorded in the task file. Remote
CI remains pending until this branch is pushed.

The completed follow-on [native-engine-067](tasks/native-engine-067.md) slice
adds bounded non-inherited signed `order` values to eligible fixed-width flex
rows. Items sort by ascending order with stable source-order ties before the
existing gap and `justify-content` distribution; semantic DOM/source order
remains unchanged. The `-1024..=1024` integer bound, visual-only behavior,
normal-flow fallback, and exclusions are recorded in the task file. The design
is `09f3b00`, the implementation is `a713b6e`, and local validation evidence is
recorded there. Remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-068](tasks/native-engine-068.md) slice
adds bounded non-inherited `align-items:flex-start|center|flex-end` to eligible
fixed-width single-row flex rows. It aligns complete visual item subtrees
within an explicit content height or the auto row's maximum item outer height
using deterministic integer offsets, while preserving horizontal and
semantic/source order. The design is `a0488ef`, the implementation is
`6b55b9c`, and local validation evidence is recorded in the task file. Remote
CI remains pending until this branch is pushed.

The completed follow-on [native-engine-069](tasks/native-engine-069.md) slice
adds bounded non-inherited `flex-direction:row|row-reverse` to eligible
fixed-width single-row flex rows. `row` remains equivalent to 068;
`row-reverse` lays the order-sorted visual sequence from the physical right
edge while preserving physical margins, gap, justification, cross-axis
alignment, shared subtree artifacts, non-negative coordinates, root horizontal
scrolling, and semantic/source order. The design is `7fea901`, the
implementation is `be11f49`, and local validation evidence is recorded in the
task file. Remote CI remains pending until this branch is pushed.

The completed follow-on [native-engine-070](tasks/native-engine-070.md) slice
adds bounded non-inherited `flex-wrap:nowrap|wrap` to the same eligible
fixed-width flex rows. `nowrap` remains equivalent to 069; `wrap` forms
deterministic physical lines from measured item outer widths and the existing
gap, reuses per-line justification, row/reverse direction, and cross-axis
alignment, and preserves complete subtree artifacts, root overflow, and
semantic/source order. The design is `8772a6a`, the implementation is
`5c16185`, and local validation evidence is recorded in the task file. Remote
CI remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-071](tasks/native-engine-071.md)
slice adds bounded non-inherited
`align-content:flex-start|center|flex-end|space-between` to wrapped flex rows.
It distributes only positive cross-axis free space in an explicit content box
after 070 line formation, preserving line membership, per-line item alignment,
shared artifact coordinates, and the default `flex-start` fallback. The design
is `71bd380`, the implementation is `cc1d602`, the final single-line coverage
test is `050d41b`, and local validation and cleanup evidence are recorded in
the task file. `stretch`, around-line distribution,
cross-axis gaps, column directions, `wrap-reverse`, and general Flexbox remain
outside the contract. Remote CI remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-072](tasks/native-engine-072.md)
slice adds bounded non-inherited `align-content:space-around` to wrapped flex
rows. It places each formed line at a deterministic integer slot center using
only positive explicit content-box remainder, preserving 071 line membership,
item alignment, shared artifact coordinates, and semantic/source order. The
design is `c8e5170`, the implementation is `a1c8b56`, and local validation and
cleanup evidence are recorded in the task file. `space-evenly`, `stretch`,
cross-axis gaps, column directions, `wrap-reverse`, and general Flexbox remain
outside the contract. Remote CI remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-073](tasks/native-engine-073.md)
slice adds bounded non-inherited `align-content:space-evenly` to wrapped flex
rows. It places equal integer slots before, between, and after formed lines
using only positive explicit content-box remainder, preserving 072 line
membership, item alignment, shared artifact coordinates, and semantic/source
order. The design is `23b62a3`, the implementation is `b4833a9`, and local
validation and cleanup evidence are recorded in the task file. The default
stack overflow in one existing large-Clap parser test is documented there;
the full native library suite passes with an explicit 8 MiB test-thread stack.
`stretch`, `place-content`, cross-axis gaps, column directions, `wrap-reverse`,
and general Flexbox remain outside the contract. Remote CI remains pending
until this branch is pushed.

The completed dependency-ordered [native-engine-074](tasks/native-engine-074.md)
slice adds bounded non-inherited `flex-wrap:wrap-reverse` to the same eligible
fixed-width flex rows. It preserves source-order line formation while placing
formed lines from the physical cross-axis end, reusing every bounded
`align-content` value and translating complete line artifact ranges with a
signed document-pixel delta. The design is `5f6c61a`, the implementation is
`96fd15c`, and local validation and cleanup evidence are recorded in the task
file. The previously documented default-stack issue in one large-Clap parser
test remains a harness follow-up; the full native library suite passes with an
explicit 8 MiB test-thread stack. Remote CI remains pending until this branch
is pushed.

The completed dependency-ordered [native-engine-075](tasks/native-engine-075.md)
slice adds bounded explicit non-inherited `align-content:stretch` to wrapped
fixed-width flex rows. It expands formed line heights by deterministic integer
shares of positive explicit content-box remainder, then reuses per-line
`align-items` and complete artifact translation for normal and wrap-reverse
stacking. The design is `4637863`, implementation is `e26c0a4` with the
diagnostics-fixture correction in `cc8b538`, and local validation and cleanup
evidence are recorded in the task file. The default-stack issue in one
existing large-Clap parser test remains a harness follow-up; the full native
library suite passes with an explicit 8 MiB test-thread stack. Remote CI
remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-076](tasks/native-engine-076.md)
slice adds the explicit non-inherited `align-content:normal` keyword to the
same wrapped fixed-width rows. In this bounded engine it aliases the completed
075 line-box stretch owner for positive explicit cross-axis remainder, while
the established omitted-value `flex-start` fallback remains unchanged. The
design is `b6c647e`, implementation is `e3933b3`, and local validation and
cleanup evidence are recorded in the task file. The default-stack issue in one
existing large-Clap parser test remains a harness follow-up; the full native
library suite passes with an explicit 8 MiB test-thread stack. Remote CI
remains pending until this branch is pushed.

The completed dependency-ordered [native-engine-077](tasks/native-engine-077.md)
slice adds explicit non-inherited `row-gap` spacing between adjacent formed
lines in eligible wrapped fixed-width flex rows. It includes that gap once in
the existing `align-content` occupied-size/free-space owner and preserves the
current main-axis-only `gap` behavior. The design is `99d70ef`, implementation
is `4b27b15`, and local validation and cleanup evidence are recorded in the
task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-078](tasks/native-engine-078.md)
slice completes the bounded flex gap family: one- and two-value integer-pixel
`gap`, `row-gap`, and `column-gap` with declaration-order-aware shorthand and
longhand cascade. One-value `gap` intentionally supplies both axes, and the
affected wrapped-row goldens move with that implementation. The design is
`c6ebecd`, implementation is `1bca33f`, and local validation and cleanup
evidence are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-079](tasks/native-engine-079.md)
slice adds bounded non-inherited integer `flex-grow` weights to the existing
row-flex owner. Positive free space is allocated before justification with a
deterministic prefix-floor policy, max-width caps freeze and redistribute
remainder, and negative free space remains an explicit no-shrink overflow
case. The design is `a45fb01`, implementation is `1a930a3`, and local
validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-080](tasks/native-engine-080.md)
slice adds bounded non-inherited integer `flex-shrink` weights to the same
row-flex owner. Negative line free space is allocated by original-base-width
weighted prefix-floor shares, effective `min-width` floors freeze and
redistribute the deficit, and zero-factor or minimum-exhausted rows retain
explicit overflow. The design is `46227de5`, implementation is `b1414931`, and
local validation and cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-081](tasks/native-engine-081.md)
slice defines bounded `flex-basis:auto|Npx` sizing for the same row-flex owner.
Explicit bases override item `width`, use the existing box-sizing and min/max
helpers, remain unclamped before line formation so the completed grow/shrink
passes can resolve them, and preserve margins, gaps, descendants, paint,
overflow, hit testing, and semantic/source order. The design is `04cc4a3e`,
implementation is `299c93f9`, and local validation and cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.
The completed dependency-ordered [native-engine-082](tasks/native-engine-082.md)
slice adds bounded `flex` shorthand expansion into the existing grow, shrink,
and basis components. It covers `none`, `auto`, bounded integer factor forms,
and bounded pixel/`auto` bases with declaration-order-aware longhand overrides;
unsupported CSS-wide, percentage, fractional, and ambiguous forms remain
diagnosed. The design is `71d1060d`, implementation is `40f6fb1c`, and local
validation and exact target-cleanup evidence are recorded in the task file.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-083](tasks/native-engine-083.md)
slice defines bounded `flex-flow` shorthand expansion into the existing
direction and wrap components. It covers row/reverse-row and nowrap/wrap/
wrap-reverse tokens in either order, with omitted components reset to their
initial values; unsupported columns, duplicates, CSS-wide, logical-direction,
and ambiguous forms remain diagnosed. The design is `80c6836e`, implementation
is `0291bf90`, the strict-Clippy fix is `16e6d9aa`, and focused/full validation
and exact target-cleanup evidence are recorded in the task file. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-084](tasks/native-engine-084.md)
slice defines bounded non-inherited `align-self:auto|flex-start|center|flex-end`
for eligible direct flex items. `auto` resolves to the existing parent
`align-items` value; explicit values reuse the existing line/subtree artifact
translation. Stretch, baseline, logical, CSS-wide, and ambiguous forms remain
diagnosed. The design is `36085e9c`, implementation is `f2f99f66`, and focused,
full, strict, documentation, and exact target-cleanup evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-085](tasks/native-engine-085.md)
slice defines bounded `place-content` expansion into the existing
`align-content` and `justify-content` components. One shared token supports
their common values; two tokens use explicit cross-axis/main-axis order.
Unsupported CSS-wide, logical, safe/unsafe, ambiguous, and unsupported justify
forms remain diagnosed. The design is `adc61a1f`, implementation is
`02f866e6`, and focused, full, strict, documentation, and cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-086](tasks/native-engine-086.md)
slice extends bounded non-inherited `align-self` with `stretch` for eligible
direct flex items. Auto-height items fill the existing line cross size while
explicit heights keep their declared size and use the bounded flex-start
fallback. The design checkpoint is `f92b7b9a`, implementation is `5ea2c8d1`,
and strict layout lint cleanup is `2c07c749`; focused, full, strict,
documentation, release-certification, and exact target-cleanup evidence are
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-087](tasks/native-engine-087.md)
slice extends parent `align-items` with explicit `stretch`. Children whose
`align-self` remains `auto` reuse the completed 086 used-size path; explicit
child overrides remain authoritative, explicit heights stay fixed, and the
omitted native fallback remains `flex-start`. Design is `4708f663`,
implementation is `e0d051ce`, and focused/full/strict/release-certificate,
documentation, and exact-target evidence are recorded in the task file. The
first all-in-one certification run exposed one environment-sensitive Rust
Analyzer probe; its exact retry passed. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-088](tasks/native-engine-088.md)
slice adds explicit parent `align-items:normal`. In the supported row and
row-reverse flex context, `normal` resolves auto-aligned children through the
completed stretch path while the computed value remains distinct and the
omitted native fallback stays `flex-start`. Design is `162543f1`, implementation
is `19d8d3f6`, and focused, full, strict, documentation, and exact-target
evidence are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-089](tasks/native-engine-089.md)
slice adds explicit child `align-self:normal`. In the supported row and
row-reverse flex context, the explicit item value reuses the completed stretch
used-size path regardless of the parent's `align-items` value, while omitted
`align-self:auto` remains parent-controlled and the computed keyword remains
distinct. Design is `95caf4a9`, implementation is `1ee55c43`, and focused,
full, strict, documentation, and exact-target evidence are recorded in the
task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-090](tasks/native-engine-090.md)
slice adds explicit `justify-content:space-around` to the bounded fixed-width
row and row-reverse flex-line owner. Positive main-axis free space is
distributed with deterministic integer cumulative offsets around the existing
item/gap/margin and flex-sizing geometry; row-reverse mirrors the offsets and
all downstream artifacts remain shared. Design is `97c69d9d`, implementation is
`d814784f`, and focused, full, strict, documentation, and exact-target evidence
are recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-091](tasks/native-engine-091.md)
slice adds explicit `justify-content:space-evenly` to the same bounded
fixed-width row and row-reverse flex-line owner. Positive main-axis free space
is distributed into deterministic equal integer slots after existing
item/gap/margin and flex-sizing geometry, while preserving shared layout,
paint, hit-test, scroll, capture, and semantic/source-order consumers. Design
is `be1a8e20`, implementation is `118590f7`, and focused, full, strict,
documentation, binary, validator, and exact-target evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-092](tasks/native-engine-092.md)
slice adds explicit `justify-content:normal` to the same bounded row and
row-reverse owner. It preserves a distinct computed keyword while routing
used placement through the completed `flex-start` geometry, and makes the
shared one-token `place-content:normal` expansion valid. Design is
`4cbaf338`, implementation is `ce19db39`, and focused, full, strict,
documentation, binary, validator, and exact-target evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-093](tasks/native-engine-093.md)
slice adds explicit `justify-content:stretch` to the same bounded row and
row-reverse owner. It preserves a distinct computed keyword while routing used
placement through the completed `flex-start` geometry, and makes the shared
one-token and two-token `place-content:stretch` forms valid. Design is
`273b31db`, implementation is `653f025e`, and focused, full, strict,
documentation, binary, validator, and exact-target evidence are recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-094](tasks/native-engine-094.md)
slice extends the bounded Flexbox owner to explicit `flex-direction:column` and
`column-reverse` for fixed-height, no-wrap containers. It maps existing
vertical main-axis justification, row-gap, grow/shrink/basis, cross-axis item
alignment, descendants, and shared artifacts without adding a second geometry
owner. Auto-height columns, wrapping, column-gap line distribution, logical
writing modes, and browser-wide Flexbox remain outside the contract. Design is
`aea47b17`, implementation checkpoints are `7dc92517` and `4e212151`, and the
focused, full, strict, documentation, validator, binary, and exact-target
evidence is recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-095](tasks/native-engine-095.md)
slice extends column and column-reverse flex to fixed-height `flex-wrap:wrap`
containers. It forms vertical main-axis lines, maps `column-gap` across the
horizontal cross axis, reuses per-line flex sizing/justification and existing
`align-content`, alignment, and shared artifact consumers. `wrap-reverse`,
auto-height columns, and browser-wide Flexbox remain outside this contract.
The design is `598228a7`, implementation is `96ea62a3`, and focused/full
native, feature-library, strict-Clippy, and exact fallback evidence is recorded
in the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-096](tasks/native-engine-096.md)
slice extends that owner to fixed-height column and column-reverse
`flex-wrap:wrap-reverse`. It reflects the formed horizontal line boxes and
cross-axis item alignment while preserving 095 line formation, gaps,
`align-content`, main-axis reversal, complete artifacts, and source/semantic
order. Auto-height columns, intrinsic or percentage sizing, logical writing
modes, and browser-wide Flexbox remain outside the contract. The design is
`f5026f3c`, implementation is `6862aff6`, and focused/full native,
feature-library, strict-Clippy, and exact fallback/cleanup evidence is recorded
in the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-097](tasks/native-engine-097.md)
slice extends the existing no-wrap row, row-reverse, column, and
column-reverse owners to bounded `margin:auto` edges. Auto margins are zero
during flex sizing, then absorb positive main-axis space before
`justify-content` and positive cross-axis space before `align-items`/
`align-self`, with deterministic integer remainder allocation and complete
artifact consumers. Wrapped lines, auto-height columns, intrinsic or
percentage sizing, and normal-flow auto margins remain outside the contract.
The design is `2901c830`, the implementation is `815794ce`, and focused/full
native, feature-library, strict-Clippy, rustdoc, binary, validator, and
exact-target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.

The completed dependency-ordered [native-engine-098](tasks/native-engine-098.md)
slice extends that owner to line-local `margin:auto` resolution for eligible
wrapped row/row-reverse and fixed-height column/column-reverse containers.
Auto edges remain zero during line formation and per-line sizing, then absorb
positive main-axis remainder before justification and positive cross-axis
remainder before item alignment after the existing line and `align-content`
owners have settled. `wrap-reverse`, reverse physical edges, deterministic
integer shares, and complete artifact consumers remain in the same geometry
path. Auto-height columns, new intrinsic/percentage sizing, normal-flow auto
margins, and browser-wide Flexbox remain outside the contract. The design is
`a4b05f07`, implementation is `77a4b629`, and the final test-only checkpoint
is `866a8862`. Focused/full native, feature-library, strict-Clippy, rustdoc,
binary, package/dependency, fuzz, documentation, and exact-target cleanup
evidence is recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-099](tasks/native-engine-099.md)
slice adds inherited `direction:ltr|rtl` to the existing bounded Flexbox axis
mapping. Rows use the current inline direction for their physical main start;
columns preserve their vertical main axis while reflecting horizontal
cross-axis alignment and wrapped line stacking. Source/semantic order,
non-flex text bidi, vertical writing modes, and browser-wide directionality
remain outside the contract. The design checkpoint is `2f18abb4`, the
implementation is `3bf658e8`, and the complete local gate evidence is recorded
in the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-100](tasks/native-engine-100.md)
slice adds bounded inherited `text-align:start|end` to the fixed-cell
inline-flow owner, resolving logical start/end through inherited
`direction:ltr|rtl` while preserving physical `left|right`, center, source
order, wrapped-line behavior, and the existing no-bidi/shaping boundary. The
design checkpoint is `2dae80fc`, the implementation is `3380978c`, and the
complete local gate and paired-package evidence is recorded in the task file.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-101](tasks/native-engine-101.md)
slice adds bounded inherited `text-align:justify` to the fixed-cell inline-flow
owner. Only eligible collapsed ASCII separators on soft-wrapped non-final
lines receive deterministic integer expansion; preformatted flow, bidi,
shaping, and the broader text-conformance matrix remain outside the contract.
The design is `959cbbc9`, implementation is `8ff29aa1`, and the explicit
word-spacing acceptance test is `15cf0c85`; the complete local gate evidence is
recorded in the task file. Exact isolated-target cleanup is recorded in the
final cleanup checkpoint. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-102](tasks/native-engine-102.md)
slice adds bounded inherited `text-align-last:auto|left|center|right|start|end`
to the same fixed-cell inline-flow owner. Only the final non-empty line flushed
by a block's normal completion path uses the explicit value; 101 soft-wrap
justification, forced-break paths, bidi/shaping, and full text conformance
remain bounded as documented. The design is `fc396200`, implementation is
`1157bf49`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-103](tasks/native-engine-103.md)
slice adds bounded inherited `text-align-last:justify` to the same fixed-cell
inline-flow owner. Only the final non-empty line flushed by a block's normal
completion path may distribute positive free space across eligible collapsed
ASCII separators; 101 soft-wrap justification and 102 physical/logical final
alignment remain bounded as documented. The design is `f261773f`, implementation
is `be5757ae`, and complete local gate plus exact-target cleanup evidence is
recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-104](tasks/native-engine-104.md)
slice adds bounded inherited `text-justify:auto|none|inter-word` to the same
fixed-cell text-spacing owner. `none` suppresses the existing separator
expansion for ordinary soft-wrap and explicit final-line justification;
`auto` and `inter-word` retain the bounded ASCII-space algorithm. The design is
`6ca523f1`, implementation is `d83b24e4`, and complete local gate plus
exact-target cleanup evidence is recorded in the task file. Remote CI remains
pending because the branch is local-only.

The completed dependency-ordered [native-engine-105](tasks/native-engine-105.md)
slice extends the inherited fixed-cell `text-decoration` owner from
`none|underline` to the bounded single values
`none|underline|overline|line-through`. The line state flows through the
existing immutable text commands, clipping, scrolling, opacity replay, and
software raster without changing layout or semantic/source order. Decoration
colors, thickness, style, offsets, combinations, font metrics, shaping, bidi,
vertical writing, and browser-wide text conformance remain outside the
contract. Design is `9002ae13`, implementation is `ebfefefa`, and complete
local gate plus exact-target cleanup evidence is recorded in the task file.
Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-106](tasks/native-engine-106.md)
slice extends the inherited fixed-cell `text-decoration` owner to distinct
multi-token combinations in the `text-decoration` shorthand. The bounded
three-bit line set flows through the existing immutable text commands,
clipping, scrolling, opacity replay, capture, and software raster without
changing layout or semantic/source order. `text-decoration-line` longhand
semantics, decoration colors, thickness, style, offsets, font metrics,
shaping, bidi, vertical writing, and browser-wide text conformance remain
outside the contract. Design is `c0525afb`, implementation is `baf680ee`,
and complete local gate plus exact-target cleanup evidence is recorded in the
task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-107](tasks/native-engine-107.md)
slice extends the inherited fixed-cell decoration owner with a local
`text-decoration-color` value. The existing bounded `NativeColor` grammar
feeds a separate decoration color beside glyph color in one immutable text
command, preserving shared geometry, clipping, scrolling, opacity, capture,
and raster consumers while separating glyph and line pixels. Explicit
decoration-origin propagation, `currentColor` syntax, decoration
style/thickness/offset, and full color/text conformance remain outside the
contract. Design is `c5177215`, implementation is `2474efe6`, and the local
144/144 native integration plus current workspace gate evidence is recorded in
the task file. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-108](tasks/native-engine-108.md)
slice exposes the same fixed-cell line bitset through the bounded
`text-decoration-line` longhand. `none`, `underline`, `overline`, and
`line-through` combinations reuse the shorthand's declaration-order-aware
slot and one immutable text command, preserving the 107 glyph/decoration color
path and all shared artifact consumers. Full CSS longhand
inheritance/decoration propagation, style/thickness/offset, and text
conformance remain outside the contract. Design is `ea1bf881`, implementation
is `4981ff82`, and complete local gate evidence is recorded in the task file;
the native integration suite passed 145/145 and the full browser/dev plus
strict docs, package, fuzz, and static gates passed. Remote CI remains pending
because the branch is local-only.

The completed dependency-ordered [native-engine-109](tasks/native-engine-109.md)
slice adds bounded inherited `text-decoration-style:solid|dashed|dotted` to
the existing fixed-cell decoration owner. Solid remains the default; dashed
and dotted reuse the existing integer border-pattern helper with one-pixel
decoration lines anchored at each emitted run origin. Wavy/double styles,
thickness, offsets, decoration-origin propagation, and browser-wide CSS
conformance remain outside the contract. Design `93034cbf`, implementation
`81069084`, inherited-style coverage `b8ae87dc`, and docs closeout are recorded
in the task file. Local native, browser/dev, package, fuzz, static, and
formatting gates passed; exact-target cleanup is recorded there. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-110](tasks/native-engine-110.md)
slice adds bounded inherited `text-decoration-thickness:1px|2px|3px|4px` to
the existing fixed-cell decoration owner. Each selected line keeps its
existing y origin and paints a positive-y pixel band; dashed and dotted
periods reuse the existing integer helper scaled by thickness, and replay
clamps externally constructed commands to the same 4px ceiling. Arbitrary,
font-derived, fractional, negative, zero, offset, baseline, and browser-wide
CSS decoration semantics remain outside the contract. Design `1623c5f1`,
implementation `157da4ad`, complete local gate evidence, and exact-target
cleanup are recorded in the task file. Remote CI remains pending because the
branch is local-only.

The completed dependency-ordered [native-engine-111](tasks/native-engine-111.md)
slice adds bounded inherited signed fixed-pixel
`text-underline-offset:-4px..=4px` to the existing fixed-cell underline owner.
Negative offsets move the underline toward decreasing y and positive offsets
toward increasing y; overline and line-through origins remain unchanged, and
the 110 thickness/style helper is reused. `auto`, percentages, fractional
values, font-derived metrics, decoration-origin propagation, and browser-wide
CSS semantics remain outside the contract. Implementation is `215b02a8`,
current-claim docs are `e76a732a`, and complete local gate and cleanup evidence
are recorded in the task file. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-112](tasks/native-engine-112.md)
slice adds inherited `text-decoration-style:double` through a dedicated
text-decoration style type and the existing immutable text/raster path: two
solid bands, each retaining the resolved `1px..=4px` thickness, separated by
one pixel. Border styling, layout, line origins, and all existing artifact
consumers remain unchanged. Implementation is `3bdd3b54`; final local gate and
cleanup evidence are recorded in the task file. Remote CI remains pending; the
checkout is local-only.

The completed dependency-ordered [native-engine-113](tasks/native-engine-113.md)
slice adds inherited `text-decoration-style:wavy` through the existing
dedicated text-decoration style type and immutable text/raster path: a
continuous eight-pixel wave with the fixed phase
`[0,1,2,1,0,-1,-2,-1]`, applying the resolved thickness at each x column.
Run-origin phase resets, underline offset, line origins, clipping, scrolling,
opacity, capture, hit testing, and semantic/source order remain shared; CSS
metric centering, fragment continuity, antialiasing, and browser-wide
conformance remain outside the contract. Implementation is `0c6a9ddc`; final
local gate and cleanup evidence are recorded in the task file. Remote CI
remains pending because the checkout is local-only.

The dependency-ordered [native-engine-114](tasks/native-engine-114.md)
implementation is complete. It
adds inherited
`text-decoration-skip-ink:auto|none` through a dedicated value in the existing
immutable text/raster path. `auto` suppresses underline and overline pixels
only where the same fixed-cell text run emits glyph ink; `none` preserves
existing replay and line-through remains unchanged. Wavy, thickness, offset,
clipping, scroll, opacity, capture, hit testing, semantics, source order, and
layout remain shared; font metrics, shaping, fragment continuity, and
browser-wide conformance remain outside the contract. Implementation is
checkpointed at `ceedf1d8` and synchronized documentation at `d276d7b1`; all
required local certification gates pass. Exact cleanup evidence is recorded in
the task file; remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-115](tasks/native-engine-115.md)
implementation is complete. It adds inherited
`text-decoration-skip-spaces:none|all` through the existing immutable
text/raster path. `all` skips decoration pixels over same-run ASCII-space
intervals, including word, letter, and final-line justification spacing, for
underline, overline, and line-through; `none` preserves replay. `start`/`end`,
Unicode whitespace, line-boundary semantics, fragment continuity, and
browser-wide text conformance remain outside the bounded design. Implementation
is checkpointed at `1c0bd484`; local native/dev, strict, package, fuzz,
security, formatting, and static certification is complete and exact cleanup is
recorded in the task file. The browser registry-backed publish dry-run passed;
the dev registry-backed verification is blocked by the immutable public
`glass-browser 0.3.14` API surface, while the local dev no-verify packaging
dry-run passed. No upload was attempted. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-116](tasks/native-engine-116.md)
implementation is complete. It extends inherited
`text-decoration-skip-spaces` with explicit `start`, `end`, and unordered
`start end` line-edge modes. The authoritative block-owned flow flush marks
the first and last text items and carries immutable provenance alongside
display-list text commands, so raster replay skips only leading/trailing
fixed-cell ASCII-space intervals while the 115 `none|all` behavior remains
unchanged; nested inline temporary flows cannot claim a line edge. Implementation
is checkpointed at `a671a559` and `db7585f7`; local native/dev, strict, package,
fuzz, security, formatting, and static certification is recorded in the task
file. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-117](tasks/native-engine-117.md)
implementation is complete at `5e65aadf`. It extends the 116 decoration
replay classifier to Rust's bounded Unicode `char::is_whitespace()` property
for literal and preformatted fixed-cell text, so tabs and non-breaking spaces
can participate in `all` or selected line-edge skipping. Normal collapsing,
line provenance, geometry, spacing arithmetic, and the explicit omission
fallback remain unchanged. Focused, full-native, two-crate, package, fuzz,
security, and static local certification passed; exact evidence and cleanup
are recorded in the task.

The dependency-ordered [native-engine-118](tasks/native-engine-118.md)
implementation is complete at `6f8e89fc`. It accepts the explicit
case-insensitive `text-decoration-skip-spaces: initial` keyword, mapping it to
the existing `start end` computed value while preserving the deliberate
omitted-property `none` fallback. General CSS-wide keyword machinery and all
layout, raster, dependency, feature-default, and crate-boundary changes remain
outside this slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task.

The dependency-ordered [native-engine-119](tasks/native-engine-119.md)
implementation is complete at `1118bf2b`. It accepts explicit,
case-insensitive `text-decoration-skip-spaces: inherit` through a private
declaration-only value resolved at the existing parent-style boundary,
keeping the public finite paint enum and all artifact consumers unchanged.
`unset`, `revert`, `revert-layer`, general CSS-wide keyword machinery,
layout/raster changes, new dependencies, default-feature changes, and
crate-boundary changes remain outside this slice. Focused, full-native,
two-crate, strict, package, fuzz, security, formatting, and static local
certification passed; exact evidence and cleanup are recorded in the task.

The dependency-ordered [native-engine-120](tasks/native-engine-120.md)
implementation is complete at `897bd648`. It accepts explicit
case-insensitive `text-decoration-skip-spaces: unset` through the same private
declaration-only value and resolves it as inherited parent state, keeping the
public finite paint enum and all artifact consumers unchanged. `revert`,
`revert-layer`, general CSS-wide keyword machinery, layout/raster changes, new
dependencies, default-feature changes, and crate-boundary changes remain
outside this slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-123](tasks/native-engine-123.md)
implementation is complete at `af644112`. It reuses the 122 layer registry
and private rollback state for inherited `text-decoration-skip-ink:
revert-layer`, preserving the finite `Auto|None` paint value,
glyph-intersection replay, and the unlayered/inline layer boundary. Parser,
cascade, display-list, and decoded-raster regressions passed; general CSS-wide
keyword machinery, multiple origins, layer statements, and unsupported values
remain outside the slice. Exact local gate and cleanup evidence is recorded in
the task; remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-124](tasks/native-engine-124.md)
implementation is complete at `d6c8bc70`. It reuses the 122 layer registry and
the private rollback boundary proven by 123 for inherited
`text-decoration-style: revert-layer`, preserving the finite
`Solid|Dashed|Dotted|Double|Wavy` paint value, existing fixed-cell pattern
replay, and the unlayered/inline layer boundary. Parser, cascade, display-list,
command, and decoded-raster regressions passed; general CSS-wide keyword
machinery, multiple origins, layer statements, and unsupported values remain
outside the slice. Exact local gate, documentation-audit, issue-sync, and
regenerable-output cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered [native-engine-125](tasks/native-engine-125.md)
implementation is complete at `271702ae`. It reuses the bounded layer registry
and private rollback boundary proven by 124 for inherited
`text-decoration-thickness: revert-layer`, preserving the finite `1px` through
`4px` value, existing one-to-four-cell decoration geometry, and the
unlayered/inline layer boundary. Parser, cascade, display-list, command, and
decoded-raster regressions passed for underline, overline, and line-through;
general CSS-wide keyword machinery, multiple origins, layer statements, and
unsupported values remain outside the slice. Exact local gate, issue-sync, and
regenerable-output cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.
The dependency-ordered [native-engine-126](tasks/native-engine-126.md)
implementation is complete at `3ffe86f8`. It reuses the bounded layer registry
and private rollback boundary proven by 125 for inherited
`text-underline-offset: revert-layer`, preserving the finite signed `-4px`
through `4px` value, existing underline-only translation, and the
unlayered/inline layer boundary. Parser, cascade, display-list, command, and
decoded-raster regressions passed; general CSS-wide keyword machinery, multiple
origins, layer statements, and unsupported values remain outside the slice.
Exact local gate, issue-sync, and regenerable-output cleanup evidence is
recorded in the task. The dependency-ordered
[native-engine-127](tasks/native-engine-127.md) implementation is complete at
`50a36545`. It reuses the bounded layer registry for local
`text-decoration-color: revert-layer`, preserving the existing
`Option<NativeColor>` no-candidate fallback and separate glyph/decoration paint
owner. Parser, cascade, display-list, command, and decoded-raster regressions
passed; general CSS-wide keyword machinery, `currentColor`, multiple origins,
layer statements, and unsupported values remain outside the slice. Exact local
gate, issue-sync, and regenerable-output cleanup evidence is recorded in the
task.

The dependency-ordered [native-engine-128](tasks/native-engine-128.md)
implementation is complete at `15fc761c`. It reuses the bounded layer registry
for case-insensitive `text-decoration-line: revert-layer` and
`text-decoration: revert-layer`, preserving their shared inherited three-bit
line-state owner, declaration-order interaction, unlayered/inline bucket, and
existing display-list, command, and fixed-cell raster geometry. Parser,
cascade, inherited fallback, and decoded-raster regressions passed; other
CSS-wide keywords, multiple origins, layer statements, and unsupported values
remain typed diagnostics. Exact local gate, documentation-audit, issue-sync,
and regenerable-output cleanup evidence is recorded in the task. Remote CI
remains pending because the checkout is local-only.

The dependency-ordered [native-engine-129](tasks/native-engine-129.md)
implementation is complete in `d26033af`, with its fixture assertion correction
in `c32aeafe` and diagnostic-classifier fix in `36a0f68`. It adds private
case-insensitive `revert-layer` declarations for inherited `text-align`,
`text-align-last`, and `text-justify`, reusing the bounded 15-layer and
unlayered/inline cascade boundary while preserving direction mapping,
final-line alignment, separator justification, finite public values, and the
existing fixed-cell line/artifact owner. Focused and full affected-package
local gates are recorded in the task; remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-130](tasks/native-engine-130.md)
implementation is complete in `d7f4d7ca`. It adds private case-insensitive
`white-space: revert-layer` declarations through the bounded 15-layer and
unlayered/inline cascade boundary. It preserves the five finite whitespace
modes, inherited/root fallback, existing hard-break and fixed-cell wrapping
behavior, and the current line/artifact owner. Focused and complete affected-
package local gates passed; the package library gate used an explicit 32 MiB
test-thread stack to accommodate one pre-existing CLI stack-overflow test.
Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-131](tasks/native-engine-131.md)
implementation is complete in `e35a13fd`. It adds private case-insensitive
`line-height: revert-layer` declarations through the bounded 15-layer and
unlayered/inline cascade boundary. It preserves the positive-pixel line-height
grammar, inherited/root `Option<u32>` fallback, inline auto-height and
explicit-height precedence, and the current flow/artifact owner. Focused and
complete affected-package local gates passed; the package library gate used an
explicit 32 MiB test-thread stack to accommodate one pre-existing CLI
stack-overflow test. Remote CI remains pending because the checkout is
local-only.

The dependency-ordered [native-engine-132](tasks/native-engine-132.md)
implementation is complete in `4a46862f`. It adds private case-insensitive
`direction: revert-layer` declarations through the bounded 15-layer and
unlayered/inline cascade boundary while preserving the finite `ltr|rtl` value,
logical text-edge mapping, flex directionality, wrapped-line placement,
source/semantic order, and current layout/artifact owners. Focused,
full-native, affected-library, and strict affected-package local gates passed;
exact test and cleanup evidence is recorded in the task. Remote CI remains
pending because the checkout is local-only.

The dependency-ordered [native-engine-133](tasks/native-engine-133.md)
implementation is complete in `56944c83`. It extends the bounded private
layer resolver to non-inherited `flex-direction: revert-layer`, preserving
finite row/row-reverse/column/column-reverse placement, the local `row`
fallback, finite `flex-flow` expansion, and the existing wrapped-flex/artifact
owners. Focused, full-native, affected-library, and strict affected-package
local gates passed; exact test and cleanup evidence is recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered [native-engine-134](tasks/native-engine-134.md)
implementation is complete in `f2e20121`. It extends the bounded private
layer resolver to non-inherited `flex-wrap`, `justify-content`, `align-items`,
`align-self`, and `align-content`, preserving their native fallbacks, finite
`flex-flow`/`place-content` expansion, and existing wrapped-flex/artifact
consumers. Focused, full-native, affected-library, and strict affected-package
local gates passed; exact test and cleanup evidence is recorded in the task.
Remote CI remains pending because the checkout is local-only.
The dependency-ordered [native-engine-135](tasks/native-engine-135.md)
implementation is complete in `74195032`. It extends the bounded private
layer resolver to non-inherited `order`, `flex-grow`, `flex-shrink`, and
`flex-basis`, preserving finite `flex` expansion, local fallbacks, stable
visual order, grow/shrink allocation, base-size selection, min/max constraints,
and existing layout/artifact consumers. Focused, full-native,
affected-library, and strict affected-package local gates passed; exact test
and cleanup evidence is recorded in the task. Remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-136](tasks/native-engine-136.md)
implementation is complete in `710ed3bb`. It adds standalone
case-insensitive `flex:revert-layer` through the existing private
grow/shrink/basis rollback components, preserving finite shorthand expansion,
same-block longhand precedence, local fallbacks, and the current
layout/artifact owners. Focused, full-native, affected-library, and strict
affected-package local gates passed; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.
The dependency-ordered [native-engine-137](tasks/native-engine-137.md)
implementation is complete in `7da4dfd5` (design `79e2d2fa`). It adds
standalone case-insensitive `flex-flow:revert-layer` and
`place-content:revert-layer` through the existing private direction/wrap and
align-content/justify-content rollback components, preserving finite shorthand
expansion, same-block longhand precedence, independent local fallbacks, and
the current flex layout/artifact owners. Focused parser/cascade, integration,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed; exact evidence and cleanup are recorded in the
task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-138](tasks/native-engine-138.md) implementation is complete in `bded96ae` (design `656dc37d`). It adds standalone case-insensitive `gap:revert-layer`, `row-gap:revert-layer`, and `column-gap:revert-layer` through private row/column component candidates, preserving finite integer-pixel expansion, same-block shorthand/longhand precedence, independent zero fallback, and the current flex layout/artifact owners. Focused parser/cascade, integration, full-native, affected-library, strict Clippy, formatting, and static documentation gates passed; exact evidence and cleanup are recorded in the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-139](tasks/native-engine-139.md)
implementation is complete in `0114659d` (design `16589ae4`). It adds
standalone case-insensitive `revert-layer` to inherited `text-transform`,
`font-weight`, `font-style`, and `word-break` through private per-property
candidates, preserving finite public values, parent/root fallback, and the
existing fixed-cell layout, wrapping, display-list, raster, overflow, capture,
hit-test, and semantic/source-order owners. Unicode case mapping, font metrics,
other word-break modes, multiple origins, and browser-wide text conformance
remain outside the boundary; exact implementation, validation, and cleanup
evidence are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-140](tasks/native-engine-140.md)
implementation is complete in `7d40cf87` (design `34f8ec1a`). It adds
standalone case-insensitive `revert-layer` to inherited `word-spacing` and
`letter-spacing` through private per-property candidates, preserving finite
non-negative pixel values, parent/root fallback, and the existing text-flow,
wrapping, alignment, display-list, raster, overflow, capture, hit-test, and
semantic/source-order owners. The shared parser also preserves an earlier
valid inherited-text declaration when a later declaration is invalid.
Negative, relative, percentage, fractional, cross-fragment, font-metric,
multi-origin, and browser-wide text semantics remain outside the boundary;
exact implementation, validation, and cleanup evidence are recorded in the
task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-141](tasks/native-engine-141.md)
implementation is complete in `271bfaf2` (design `0d1f7281`) and recorded in
the task. It adds standalone case-insensitive `revert-layer` to inherited
`vertical-align` through private candidates, preserving finite
`baseline|top|middle|bottom` values, parent/root fallback, and the existing
inline line-item, text-fragment, display-list, raster, overflow, capture,
hit-test, and semantic/source-order owners. Baseline metrics, lengths,
percentages, bidi, writing modes, multiple origins, and browser-wide text
conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote
CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-142](tasks/native-engine-142.md)
implementation is complete in `be860447` (design `2794365f`) and recorded in
the task. It adds standalone case-insensitive `revert-layer` to the local
`text-indent` and `text-overflow` owners through private candidates, preserving
finite non-negative fixed-pixel indentation, `clip|ellipsis`, local `0px`/`clip`
fallbacks, and the existing first-line flow, eligible clipped-nowrap
truncation, text-fragment, display-list, raster, overflow, capture, hit-test,
and semantic/source-order owners. Negative or hanging indentation, percentages,
font-relative units, inherited text-overflow, marker customization, multiple
origins, and browser-wide text conformance remain outside the boundary.
Focused, full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-143](tasks/native-engine-143.md)
implementation is complete in `b55751da` (design `edc29d7c`) and recorded in
the task. It adds standalone case-insensitive `revert-layer` to the local
`width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`
owners through independent private candidates, preserving finite non-negative
pixel dimensions, absent local fallbacks, and the existing box-model,
normal-flow, flex, overflow, capture, hit-test, display-list, raster, and
semantic/source-order owners. Percentages, negative dimensions, intrinsic
sizing, aspect ratio, multiple origins, and browser-wide CSS sizing conformance
remain outside the boundary. Focused, full-native, affected-library, strict
Clippy, formatting, and static documentation gates passed locally; exact
evidence and cleanup are recorded in the task. Remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-144](tasks/native-engine-144.md)
implementation is complete in `7ce9c52c` (design `255a1ac8`) and recorded in
the task. It extends the same private bounded layer resolver to standalone
case-insensitive `revert-layer` for local `box-sizing`, physical padding and
margin edges, including shorthand/longhand rollback and bounded `margin:auto`,
preserving independent edge ownership, content-box/zero local fallbacks, and
the existing box-model, normal-flow, flex, overflow, capture, hit-test,
display-list, raster, and semantic/source-order owners. Percentages,
negative/logical edges, margin collapsing, positioned or replaced-element
sizing, multiple origins, `!important` inversion, layer statements, and
browser-wide box-model conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, formatting, and static
documentation gates passed locally; exact evidence and cleanup are recorded in
the task. Remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-145](tasks/native-engine-145.md)
implementation is complete in `997d4aa7` (design `553c5f89`) and is recorded
in the task. It reuses the same private bounded layer resolver for standalone
case-insensitive `revert-layer` on local `background-color` and inherited
`color`, preserving independent `None`/inherited fallbacks and the existing
fill/text display-list, capture, raster, clipping, opacity, hit-test, and
semantic/source-order owners. Border-color, `currentColor`, gradients, system
colors, multiple origins, and browser-wide CSS color conformance remain
outside the boundary. Focused, full-native, affected-library, strict Clippy,
formatting, and static documentation gates passed locally; exact evidence and
cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-146](tasks/native-engine-146.md)
implementation is complete in `462d2a70` (design `d1cd1eab`) and is recorded
in the task. It reuses the same private bounded layer resolver for standalone
case-insensitive `revert-layer` on local `overflow`, `overflow-x`, and
`overflow-y`, preserving independent x/y candidates, the existing visible
fallback, and the shared paint, viewport projection, point-hit, root-overflow,
capture, and semantic/source-order owners. Nested scrolling, scrollbars,
`visible`/`auto`/`scroll` used-value parity, multiple origins, and browser-wide
CSS overflow conformance remain outside the boundary. Focused, full-native,
affected-library, strict Clippy, formatting, and static documentation gates
passed locally; exact evidence and cleanup are recorded in the task. Remote CI
remains pending because the checkout is local-only.

The dependency-ordered [native-engine-147](tasks/native-engine-147.md)
implementation is complete in `74cc1cf9` (design `f76720f7`). It reuses the
same private bounded layer resolver for standalone case-insensitive
`revert-layer` on the local bounded one-to-four-value integer `border-radius`
shorthand, preserving the zero-corner fallback and the existing rounded fill,
border, point-hit, capture, raster, overflow, and semantic/source-order
owners. Elliptical, percentage, corner-longhand, nested-clip, anti-aliasing,
multiple-origin, and browser-wide border-radius conformance remain outside the
boundary. Focused, full-native, affected-library, strict Clippy, formatting,
and static documentation gates passed locally; exact evidence and cleanup are
recorded in the task. Remote CI remains pending because the checkout is
local-only.
The dependency-ordered [native-engine-148](tasks/native-engine-148.md)
implementation is complete in `d3f89a6c` (design `d882d846`). It reuses the
same private bounded layer resolver for standalone, case-insensitive
`revert-layer` on the existing local 8-bit `opacity` owner, preserving the
full-opacity fallback, reduced-opacity group markers and software
compositing, and the existing layout, point-hit, capture, raster, overflow,
and semantic/source-order owners. Inherited opacity, stacking-context/blending
parity, filters, animation, multiple origins, and browser-wide opacity
conformance remain outside the boundary. Focused parser/cascade and
integration tests, full-native integration/library tests, strict
affected-package Clippy, and formatting passed locally. Static documentation
gates passed with 562 Markdown documents, 83 current documents, 57
previous-version hits, 656 semantic-audit hits, and 0 current-claim failures.
Documentation coverage passed with 562 Markdown files, 345 full-product MCP
tools (100 browser-only), 17 examples, and 22 public modules; depth passed
with 93 guides and 19 substantive contracts; parity passed for 14 capabilities
across 4 targets; TUI passed at 15 implementation help keys/63 documentation
markers; adapters passed at 5; reliability passed at 6 scenarios across 4
targets; and Web IR passed at 8 fixtures/8 scenarios/11 categories. Remote CI
remains pending because the checkout is local-only.

The dependency-ordered [native-engine-149](tasks/native-engine-149.md)
implementation is complete in `6a7dc305`, with the diagnostic compatibility fix
in `3072f6e5` (design `bd3b87b3`). It reuses the same private bounded layer
resolver for standalone, case-insensitive `revert-layer` on the existing local
`display` and `visibility` owners, preserving the normal-flow
`display:auto`/visible fallbacks and the existing hidden-subtree, normal-flow,
point-hit, display-list, capture, raster, and semantic/source-order owners.
Inherited visibility, display decomposition, formatting-context parity,
table/ruby/flow-root details, animation, multiple origins, and browser-wide
CSS display/visibility conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, feature rustdoc, formatting, and
static documentation gates are recorded in the task; remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-150](tasks/native-engine-150.md)
implementation is complete in `1fdbe75d` (design `27cf6c1a`). It reuses the
same private bounded layer resolver for standalone, case-insensitive
`revert-layer` on the existing physical `border`, `border-top`, `border-right`,
`border-bottom`, and `border-left` owners, preserving the zero-width/no-paint
fallback and the existing box-model inset, border display-list, capture,
raster, point-hit, and semantic/source-order owners. Logical sides, border-
image, gradients, other border styles, animation, multiple origins, and
browser-wide CSS border conformance remain outside the boundary. Focused,
full-native, affected-library, strict Clippy, feature rustdoc, formatting, and
static documentation gates are recorded in the task; remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-151](tasks/native-engine-151.md)
implementation is complete in `c26482b7` (design `4f23d85a`). It adds bounded
physical `border-color` and `border-top|right|bottom|left-color` shorthand/
longhands with one-to-four-value expansion, independent private per-side color
candidates, same-block declaration order, and case-insensitive
`revert-layer` rollback to lower colors or bounded black. Existing border
width/style, zero-width/no-paint, box-model, display-list, capture, raster,
point-hit, and semantic/source-order owners remain unchanged. Focused,
full-native, affected-library, strict Clippy, rustdoc, two-crate, formatting,
and static local gates are recorded in the task; remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-152](tasks/native-engine-152.md)
implementation is complete in `7dfcc7f5` (design `ff4d7803`). It adds bounded
physical `border-width` and `border-top|right|bottom|left-width` shorthand/
longhands with one-to-four-value expansion, an independent private per-side
width stream, and case-insensitive `revert-layer` rollback to lower widths or
bounded zero. Width-only declarations do not invent a style or paint a border;
existing border style/color, box-model, display-list, capture, raster,
point-hit, and semantic/source-order owners remain unchanged. Focused,
full-native, affected-library, strict Clippy, rustdoc, two-crate, formatting,
and static local gates are recorded in the task; remote CI remains pending
because the checkout is local-only.

The dependency-ordered [native-engine-153](tasks/native-engine-153.md)
implementation is complete in `6169cabc` (design `77d81fdc`). It adds bounded
physical `border-style` and `border-top|right|bottom|left-style`
shorthand/longhands with one-to-four-value expansion, an independent private
per-side style stream, same-block declaration order, and case-insensitive
`revert-layer` rollback. Resolved width, style, and color components compose
only after independent resolution; width-only or style-only declarations do
not invent missing paint components. Existing zero-width/no-paint, box-model,
display-list, capture, raster, point-hit, and semantic/source-order owners
remain unchanged. The task records focused/full-native/library, strict Clippy,
rustdoc, two-crate, formatting, static documentation, and bounded cleanup
evidence; remote CI remains pending because the checkout is local-only.

The dependency-ordered [native-engine-154](tasks/native-engine-154.md)
implementation is complete in `875cdad8` (design `e7c9ad40`). It adds
explicit physical `border-style:none` to the one-to-four-value shorthand and
four physical style longhands through a private no-paint sentinel. A winning
`none` blocks lower styles and converts to the existing no-side/zero-width
behavior before layout and artifacts; the public paint enum and display-list
schema remain unchanged. The task records focused/full-native/library, strict
Clippy, rustdoc, two-crate, formatting, static documentation, and bounded
cleanup evidence; remote CI remains pending because the checkout is local-only.
`hidden`, other border styles, logical sides, `currentColor`, gradients,
border-image, and browser-wide border conformance remain outside the boundary.

The dependency-ordered [native-engine-155](tasks/native-engine-155.md)
implementation is complete in `2b07f109` (design `37126fa1`). It adds explicit
physical `border-style:hidden` to the bounded one-to-four-value shorthand and
four physical style longhands through a distinct private no-paint sentinel. In
the current non-table engine, a winning `hidden` blocks lower styles and uses
the same no-side/zero-width result as `none`, while preserving a private
distinction for future collapsed-table conflict resolution. Public enums and
display-list schemas remain unchanged; the task records focused/full-native/
library, strict Clippy, rustdoc, two-crate, formatting, static documentation,
and bounded cleanup evidence; remote CI remains pending because the checkout is
local-only. Table conflict resolution and other border styles remain outside
the boundary.

The dependency-ordered [native-engine-156](tasks/native-engine-156.md)
implementation is complete in the current local checkpoint. It batches the
painted physical styles `double`, `groove`, `ridge`, `inset`, and `outset`
through the bounded style parser, public computed paint enum, and deterministic
software replay. Integer-pixel double stripes and two-tone/edge-directed
shading are explicit native rules; the public display-list shape remains stable
and no browser-fidelity claim is made. Logical sides, table conflict
resolution, gradients, border images, and other general CSS border conformance
remain outside the boundary. Its complete local evidence is recorded in the
task file; remote CI remains pending because the checkout is local-only.

The completed dependency-ordered [native-engine-157](tasks/native-engine-157.md)
slice is implemented at `fb2c56a2` from design `ab9d6628`. It accepts only
exact case-insensitive omitted-component `none` in the complete and physical
border shorthands, routes it through a private declaration wrapper into the
existing no-paint style stream, and preserves the current public and artifact
schemas. Width and color do not receive synthetic candidates, so a winning
`none` blocks paint while a later bounded `revert-layer` can expose an existing
lower painted component. Arbitrary omitted-component defaults, `border:hidden`,
CSS-wide resets, table conflict resolution, and browser-wide border conformance
remain outside the boundary. Focused/full-native/library, strict Clippy,
rustdoc, two-crate, formatting, and static documentation gates passed locally;
exact target cleanup is recorded in the task. Remote CI remains pending because
the branch is local-only.

The completed dependency-ordered [native-engine-158](tasks/native-engine-158.md)
slice is implemented at `f04623fc` from design `6041a479`. It accepts only
exact case-insensitive omitted-component `hidden` in the complete and physical
border shorthands, routes it through the existing private hidden style stream,
preserves the private distinction needed for future table conflict resolution,
and keeps the current public and artifact schemas unchanged. Width and color
do not receive synthetic candidates, so a winning `hidden` blocks paint while
a later bounded `revert-layer` can expose an existing lower painted component.
Arbitrary omitted-component defaults, CSS-wide resets, logical sides, table
conflict resolution, and browser-wide border conformance remain outside the
boundary. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
formatting, and static documentation gates passed locally; exact target cleanup
is recorded in the task. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-159](tasks/native-engine-159.md)
slice is implemented at `329b3cfb` from design `bdbdb208`. It accepts bounded
complete `Npx hidden color` values for the complete and physical border
shorthands, preserves declared width/color as private component candidates,
maps only style to the existing private hidden sentinel, and keeps
public/artifact schemas unchanged. Width and color cannot resurrect a hidden
side in current non-table composition, while a later bounded `revert-layer`
can expose a lower painted style with the retained complete components.
Arbitrary omitted defaults, CSS-wide resets, logical sides, table conflict
resolution, and browser-wide border conformance remain outside the boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting, and
static documentation gates passed locally; exact target cleanup is recorded in
the task. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-160](tasks/native-engine-160.md)
slice is implemented at `a880c570` from design `bf1a7236`. It accepts bounded
complete `Npx none color` values for `border` and the four physical border
shorthands, carries declared width/color through private component candidates,
and projects only the existing private `None` style sentinel. Current
non-table composition remains no-paint/no-side and public computed/artifact
schemas remain unchanged. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, formatting, static documentation, and bounded cleanup gates passed
locally; remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-161](tasks/native-engine-161.md)
slice is implemented at `299de40d` from design `c5a58f29`, with test-lint
follow-up `2042fb3e`. It adds standalone physical `border-color` and
`border-top|right|bottom|left-color` `currentColor` substitution through
private color state resolved from the existing local or inherited element
color, preserving public and artifact schemas. Complete border shorthands with
`currentColor`, broader CSS color syntax, and browser-wide conformance remain
outside the boundary. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, formatting, static documentation, and bounded cleanup gates passed
locally; exact evidence and cleanup are recorded in the task. Remote CI
remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-162](tasks/native-engine-162.md)
slice is implemented at `44b887c4` from design `4272f0fa`. It extends private
deferred color state to complete physical `Npx <style> currentColor` values for
painted, `none`, and `hidden` border forms, resolving to concrete public border
colors while preserving current no-paint behavior. Omitted defaults, broader
CSS color syntax, and browser-wide conformance remain outside the boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, formatting,
static documentation, and bounded cleanup gates passed locally; exact evidence
is recorded in the task. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-163](tasks/native-engine-163.md)
slice is implemented at `7a406855` from design `764e0c86`. It extends private
deferred-color state to `background-color: currentColor`, resolving from the
element's local or inherited `color` at computed-style construction while
preserving the public `Option<NativeColor>` fill surface and existing
layout/display-list/capture/raster/hit/semantic consumers. `color: currentColor`,
gradients, images, system colors, color spaces, percentages, CSS-wide reset
machinery, and browser-wide color conformance remain outside the boundary.
Focused/full-native/library, strict Clippy, rustdoc, two-crate, package,
formatting, static documentation, workspace all-target/all-feature, and bounded
cleanup gates passed locally; exact evidence is recorded in the task. Remote CI
remains pending because the branch is local-only.
The completed dependency-ordered [native-engine-164](tasks/native-engine-164.md)
slice is implemented at `cceb61bf` from design `e11821c5`; the diagnostics
follow-up is `005083c3` and synchronized product documentation is `2960ecc5`.
It adds case-insensitive local `text-decoration-color: currentColor` through
private deferred decoration state, resolving against the element's local or
inherited `color` while preserving the public optional concrete color,
separate glyph/decoration paint owners, and all existing text artifact
consumers. Focused/full-native/library, strict Clippy, rustdoc, two-crate,
package, formatting, static documentation, workspace all-target/all-feature,
and bounded cleanup gates passed locally; exact evidence is recorded in the
task. `color: currentColor`, gradients, images, system colors, color spaces,
percentages, animations, multiple origins, and browser-wide text-color
conformance remain outside the completed boundary. Remote CI remains pending
because the branch is local-only.
The completed dependency-ordered [native-engine-165](tasks/native-engine-165.md)
slice is implemented at `f7b5fd4e` from design `75544c39`; synchronized product
documentation is `01438316`. It adds local `color: currentColor` by resolving
the self-reference from the already-computed inherited color or bounded
initial black fallback, preserving the optional public color value and existing
background/border/text consumers. Focused/full-native/library, strict Clippy,
rustdoc, two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence and cleanup are recorded in the task. Gradients, images, system
colors, color spaces, percentages, custom-property graphs, CSS-wide reset
machinery beyond existing `revert-layer`, multiple origins, animation, and
browser-wide color conformance remain outside the completed boundary. Remote
CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-166](tasks/native-engine-166.md)
slice is implemented at `b57ba2b8` from design `d148f766`; synchronized product
documentation is `d26a9059`. It adds bounded case-insensitive `inherit`,
`unset`, `initial`, and one-author-origin `revert` handling to inherited
`color`, resolving inherited forms through the bounded parent-color/black-root
fallback and resetting `initial` to black while preserving the concrete public
value and current paint consumers. Focused/full-native/library, strict Clippy,
rustdoc, two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence is recorded in the task. Multiple origins, `!important` inversion,
gradients, images, system colors, color spaces, percentages, custom-property
graphs, animation, and browser-wide color conformance remain outside the
completed boundary. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-167](tasks/native-engine-167.md)
slice is implemented at `1ae1f103` from design `9e143200`; synchronized
product documentation is `debd6ab4`. It adds exact case-insensitive
`inherit`, `unset`, `initial`, and one-author-origin `revert` handling to
non-inherited `background-color`: only `inherit` copies the parent's optional
concrete fill, while reset forms and omission preserve the existing no-fill
`None` fallback. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence is recorded in the task. Multiple origins, `!important` inversion,
gradients, images, system colors, color spaces, percentages, custom-property
graphs, animation, and browser-wide background conformance remain outside the
completed boundary. Remote CI remains pending because the branch is local-only.

The completed dependency-ordered [native-engine-168](tasks/native-engine-168.md)
slice is implemented at `4521f151` from design `0bb67e8b`; synchronized
product documentation is `2184d98`. It adds exact case-insensitive `inherit`,
`unset`, `initial`, and one-author-origin `revert` handling to local
`text-decoration-color`, with only explicit `inherit` copying the parent's
effective concrete decoration color and reset forms resolving to the current
element color. Omission remains the public `None`/glyph-color fallback while
`currentColor`, `revert-layer`, separate glyph/line paint, and all text artifact
owners remain unchanged. Focused/full-native/library, strict Clippy, rustdoc,
two-crate, package, formatting, static documentation, workspace
all-target/all-feature, and bounded cleanup gates passed locally; exact
evidence is recorded in the task. Multiple origins, `!important` inversion,
gradients, images, system colors, color spaces, percentages, custom-property
graphs, animation, and browser-wide text-decoration conformance remain outside
the completed boundary. Remote CI remains pending because the branch is
local-only.

The completed dependency-ordered [native-engine-169](tasks/native-engine-169.md)
slice is implemented at `4d9e6979` from design `b88177dc`; test-fixture
corrections are `a0e77c84` and `801f7b19`, and synchronized product
documentation is `07ba3dc4`. It adds exact case-insensitive `inherit`, `unset`,
`initial`, and one-author-origin `revert` handling to physical `border-color`
and its four color longhands. Explicit `inherit` copies the parent's effective
per-side colors, reset forms resolve to the current element color, and
omission retains the black side fallback; `currentColor`, `revert-layer`,
border geometry, and all artifact consumers remain intact. Focused/full-native/
library, strict, package, static, workspace, and cleanup gates passed locally;
the plain `glass-dev` package path remains a known registry API mismatch while
the patched local-release archive is exact. Remote CI remains pending because
the checkout is local-only.

The completed dependency-ordered [native-engine-170](tasks/native-engine-170.md)
slice is implemented at `cf19800f` from the docs-first design in `e9a77fc0`.
It adds the same bounded case-insensitive CSS-wide family to physical
`border-width` and its four width longhands, with only explicit `inherit`
copying effective parent side widths, including unpainted and zero-width
parents, and reset/omission resolving to bounded zero. Existing
`revert-layer`, style/color composition, border geometry, and all artifact
consumers remain unchanged. Focused/full-native/library, strict, package,
static, workspace, and bounded cleanup gates passed locally; exact evidence is
recorded in the task. Remote CI remains pending because the checkout is
local-only.

The completed dependency-ordered [native-engine-171](tasks/native-engine-171.md)
slice is implemented at `67e04c0d`. It adds the same bounded case-insensitive
CSS-wide family to physical `border-style` and its four style longhands, with
only explicit `inherit` copying effective parent styles, including private
`none`/`hidden` and styles from unpainted or zero-width parents, and reset/
omission preserving the private no-style fallback. Focused/full-native/library,
strict, package, static, workspace, and cleanup gates passed locally; exact
evidence is recorded in the task. Remote CI remains pending because the
checkout is local-only.

The completed dependency-ordered [native-engine-172](tasks/native-engine-172.md)
slice is implemented at `b0bbe45a`. It adds the same bounded case-insensitive
CSS-wide family to the physical `border-radius` shorthand, copying only
explicit effective parent radii while reset and ordinary omission retain the
default zero-corner fallback. Existing bounded one-to-four-value expansion,
`revert-layer`, rounded geometry, display replay, raster, point-hit, capture,
and semantic/source-order owners remain unchanged. Focused/full-native/library
tests pass locally; remaining certification evidence is recorded in the task.
Mixed CSS-wide/concrete or slash-separated radii, corner longhands, elliptical
and percentage radii, multiple origins, and browser-wide border conformance
remain outside the completed boundary. Remote CI remains pending because the
checkout is local-only.
The completed dependency-ordered [native-engine-173](tasks/native-engine-173.md)
slice is implemented at `f5f53cec`. It adds exact case-insensitive `inherit`,
`unset`, `initial`, and one-author-origin `revert` to the complete physical
`border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
shorthands by projecting private values into the existing width/style/color
candidate streams. Explicit `inherit` copies effective parent side values;
reset forms project zero-width, private `none`, and `currentColor` so later
component declarations can compose; ordinary omission remains omission and
mixed CSS-wide/concrete forms remain unsupported. Existing concrete,
omitted-component, `currentColor`, `revert-layer`, geometry, display/raster,
capture, hit, and semantic owners remain unchanged. Focused/full-native/library,
strict Clippy, warning-denied rustdoc, paired-crate check/build, packaging,
static documentation, and workspace all-target/all-feature gates pass locally;
exact evidence is recorded in the task. Logical sides, table conflict
resolution, multiple origins, `!important` inversion, and browser-wide border
conformance remain outside the completed boundary. Remote CI remains pending
because the checkout is local-only.

The completed dependency-ordered [native-engine-174](tasks/native-engine-174.md)
slice is implemented at `f6953813`, with the strict-cascade cleanup at
`23b09864`. It adds bounded horizontal-tb logical border shorthands and their
width/style/color component families, mapping block start/end to physical
top/bottom and inline start/end through the resolved inherited ltr/rtl
`direction` owner. Logical and physical declarations compete in the existing
private physical component streams, preserving layer/source-order precedence,
`currentColor`, CSS-wide values, `revert-layer`, and all existing box-model,
display, capture, raster, point-hit, and semantic owners. Vertical writing
modes, logical radius, border images, tables, multiple origins, and
browser-wide logical-border conformance remain outside the boundary. Focused,
full-native/library, strict Clippy, warning-denied rustdoc, paired-crate,
formatting, and remaining local certification evidence is recorded in the task;
remote CI remains pending because the checkout is local-only.

The completed dependency-ordered [native-engine-175](tasks/native-engine-175.md)
slice is implemented at `2b082ddf`, with the resolver/test-shape correction at
`e6f3259d`. It adds the four physical `border-radius` corner longhands through
private per-corner candidate streams so shorthand, corner-longhand, CSS-wide,
`revert-layer`, source-order, and inherited-fallback contracts resolve before
the unchanged rounded layout, display, capture, raster, point-hit, and
semantic owners. Focused/full-native/library, strict Clippy, warning-denied
rustdoc, paired-crate check/build, packaging, static documentation, workspace
all-target/all-feature, security/fuzz, and formatting gates pass locally;
exact evidence and bounded cleanup are recorded in the task. Logical corner
names, writing-mode-dependent mapping, percentages, elliptical radii, and
browser corner fidelity remain outside the completed boundary. Remote CI
remains pending because the checkout is local-only.
The completed dependency-ordered [native-engine-176](tasks/native-engine-176.md)
slice is implemented at `6543b2b6`. It adds the four logical
`border-start-start-radius`, `border-start-end-radius`,
`border-end-start-radius`, and `border-end-end-radius` longhands, projecting
through the resolved horizontal-tb ltr/rtl direction into the existing
physical per-corner streams and rounded consumers. Focused/full-native/library,
strict Clippy, warning-denied rustdoc, paired-crate check/build, packaging,
static documentation, workspace all-target/all-feature, security/fuzz, and
formatting gates pass locally; exact evidence and bounded cleanup are recorded
in the task. Vertical writing modes, text orientation, percentages, elliptical
radii, and browser logical-radius fidelity remain outside the completed
boundary. Remote CI remains pending because the checkout is local-only.

The completed dependency-ordered [native-engine-177](tasks/native-engine-177.md)
slice is implemented at `46f6499a`. It extends the complete physical and
horizontal-tb logical radius family with bounded author-origin `!important`
priority: important candidates use a private reversed named-layer partition
and outrank normal radius candidates, while the value grammar, rounded
consumers, public schemas, and two-crate boundary remain unchanged. Focused,
full-native, feature-library, strict Clippy, and warning-denied rustdoc gates
pass locally; exact slice evidence is recorded in the task. The issue-level
workspace/release/security gates, cleanup, and remote CI remain pending until
issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-178](tasks/native-engine-178.md)
slice is implemented at `1292538c`. It extends bounded author-origin
`!important` priority to the local `background-color`, inherited `color`, and
`text-decoration-color` owners through the private reversed named-layer
partition, while preserving existing color grammar, fallback, inheritance,
paint artifacts, public schemas, and the two-crate boundary. Focused,
full-native, feature-library, strict Clippy, and warning-denied rustdoc gates
pass locally; exact slice evidence and task-specific cleanup are recorded in
the task. Issue-level final gates, final cleanup, and remote CI remain pending
until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-179](tasks/native-engine-179.md)
slice is implemented at `ed1cda27`. It extends bounded author-origin
`!important` priority to the standalone physical `border-color` shorthand and
four physical color longhands through the private reversed named-layer
partition, while preserving four-side composition,
currentColor/CSS-wide/revert-layer behavior, border width/style/layout/
artifacts, public schemas, and the two-crate boundary. Complete/side border
shorthands and logical border-color remain outside this small priority slice.
Focused, full-native, feature-library, strict Clippy, and warning-denied
rustdoc gates pass locally; final documentation audit and task-specific
cleanup are recorded in the task. Issue-level final gates and remote CI remain
pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-180](tasks/native-engine-180.md)
slice is implemented at `491f65fe`, with the strict-lint follow-up at
`100d1888`. It extends bounded author-origin `!important` priority to the six
supported horizontal-tb logical border-color declarations. It preserves the
existing `ltr`/`rtl` projection into physical sides while carrying the private
reversed named-layer partition through projection. Complete border shorthands,
width/style, vertical writing modes, and other properties remain outside this
focused slice. Focused, full-native, feature-library, strict Clippy, and
warning-denied rustdoc gates pass locally; final documentation audit and
task-specific cleanup are recorded in the task. Issue-level final gates and
remote CI remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-181](tasks/native-engine-181.md)
slice is implemented at `436dd02a`. It extends bounded author-origin
`!important` priority to the standalone physical `border-width` shorthand and
four physical width longhands while preserving one-to-four-value expansion,
independent per-side resolution, and the existing width/style/color consumers.
Complete/side border shorthands, logical border width, border style/color,
vertical writing modes, and other properties remain outside this slice.
Focused, full-native, feature-library, strict Clippy, warning-denied rustdoc,
current documentation audits, and task-specific cleanup pass locally;
issue-level final gates and remote CI remain pending until issue #40 reaches
its final validation boundary.

The completed dependency-ordered [native-engine-182](tasks/native-engine-182.md)
slice is implemented at `008a5766`. It extends bounded author-origin
`!important` priority to the standalone physical `border-style` shorthand and
four physical style longhands while preserving one-to-four-value expansion,
independent per-side no-paint/paint resolution, and the existing
width/style/color consumers. Complete/side border shorthands, logical border
style, border width/color, vertical writing modes, and other properties remain
outside this slice. Focused, full-native, feature-library, strict Clippy,
warning-denied rustdoc, current documentation audits, and task-specific cleanup
pass locally; issue-level final gates and remote CI remain pending until issue
#40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-183](tasks/native-engine-183.md)
slice is implemented at `6013d0c5`. It extends bounded author-origin
`!important` priority to the six supported horizontal-tb logical border-width
declarations, preserving their resolved `ltr`/`rtl` projection into physical
width streams. Complete/side border shorthands, logical border style, vertical
writing modes, and other properties remain outside this slice. Focused,
full-native, feature-library, strict Clippy, warning-denied rustdoc, current
documentation audits, and task-specific cleanup pass locally; issue-level
final gates and remote CI remain pending until issue #40 reaches its final
validation boundary.

The completed dependency-ordered [native-engine-184](tasks/native-engine-184.md)
slice is implemented at `26fd347a`. It extends bounded author-origin
`!important` priority to the six supported horizontal-tb logical border-style
declarations, preserving private `none`/`hidden` behavior and their resolved
`ltr`/`rtl` projection into physical style streams. Complete/side border
shorthands, logical border width/color, vertical writing modes, and other
properties remain outside this slice. Focused, full-native, feature-library,
strict Clippy, warning-denied rustdoc, current documentation audits, and
task-specific cleanup pass locally; issue-level final gates and remote CI
remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-185](tasks/native-engine-185.md)
slice is implemented at `bfcb6dc9`. It extends bounded author-origin
`!important` priority to the complete physical `border` shorthand and four
physical side-border shorthands by carrying their importance through the
existing private width/style/color component streams. Independent
side/component composition, CSS-wide and omitted `none`/`hidden`/`revert-layer`
behavior, physical layout/paint consumers, public schemas, and the two-crate
boundary remain unchanged. Logical complete shorthands, vertical writing
modes, and browser-wide border conformance remain outside this slice. Focused,
full-native, feature-library, strict Clippy, warning-denied rustdoc, current
documentation audits, and task-specific cleanup pass locally; issue-level
final gates and remote CI remain pending until issue #40 reaches its final
validation boundary.

The completed dependency-ordered [native-engine-186](tasks/native-engine-186.md)
slice is implemented at `274441e1`, with the strict-lint helper correction at
`758b891a`. It extends bounded author-origin `!important` priority to the six
supported horizontal-tb logical complete/side border shorthands by carrying
one private importance bit per logical side into the existing doubled
width/style/color component streams before resolved `ltr`/`rtl` projection.
Independent component composition, CSS-wide and omitted `none`/`hidden`/
`revert-layer` behavior, physical layout/paint consumers, public schemas, and
the two-crate boundary remain unchanged. Vertical writing modes and
browser-wide logical-border conformance remain outside this slice. Focused,
full-native, feature-library, strict Clippy, warning-denied rustdoc, current
documentation audits, and task-specific cleanup pass locally; issue-level
final gates and remote CI remain pending until issue #40 reaches its final
validation boundary.

The completed dependency-ordered [native-engine-187](tasks/native-engine-187.md)
slice is implemented at `65883117`. It extends the bounded author-origin
`!important` partition to the supported text-flow and text-decoration
declarations, preserving important-over-normal ordering, reversed named-layer
priority, inline important precedence, invalid-later preservation, inherited
and local fallbacks, and `revert-layer` rollback through the existing layout,
decoration, raster, capture, hit, diagnostics, and schema owners. Focused,
full-native, feature-library, strict Clippy, warning-denied rustdoc, current
documentation audits, and task-specific cleanup pass locally; issue-level
final gates and remote CI remain pending until issue #40 reaches its final
validation boundary.

The completed dependency-ordered [native-engine-188](tasks/native-engine-188.md)
slice is implemented at `ca0b47bd`. It extends bounded author-origin
`!important` priority to local `display`, `visibility`, and `opacity` through a
private doubled local cascade partition, preserving important-over-normal
ordering, reversed named-layer priority, inline important precedence,
invalid-later preservation, and `revert-layer` rollback through the existing
hidden-subtree, semantic, layout, display-list, raster, capture, and point-hit
owners. Flex/gap, dimensions/box model, overflow, other properties,
dependencies, multiple origins, transitions, animations, vertical writing
modes, and browser-wide CSS conformance remain outside this slice. Scoped
check, focused/full-native integration, strict Clippy, warning-denied rustdoc,
and formatting pass locally; issue-level final gates and remote CI remain
pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-189](tasks/native-engine-189.md)
slice is implemented at `94724ab0`. It extends bounded author-origin
`!important` priority to the normal-only flex and gap declarations through
private doubled flex candidate arrays and important-aware gap partitions.
`place-content`, `flex-flow`, and `flex` carry one declaration's priority to
their bounded component expansions; important-over-normal ordering, reversed
named-layer priority, inline precedence in the unlayered important bucket,
invalid-later preservation, independent gap-axis source order, and
`revert-layer` rollback flow through the existing layout and artifact owners.
Dimensions/box model, overflow priority, other properties, dependencies,
multiple origins, transitions, animations, vertical writing modes, and
browser-wide CSS conformance remain outside this slice. Scoped check, focused
units/integration, full native integration (227/227), strict Clippy,
warning-denied rustdoc, and formatting pass locally; final issue-level gates
and remote CI remain pending until issue #40 reaches its final validation
boundary.

The completed dependency-ordered [native-engine-190](tasks/native-engine-190.md)
slice is implemented at `d5cc6e6b`. It extends bounded author-origin
`!important` priority to the six normal-only local dimension declarations
through private doubled candidate arrays and per-property importance bits.
Important-over-normal ordering, reversed named-layer priority, inline
precedence in the unlayered important bucket, invalid-later preservation,
independent dimension streams, and `revert-layer` rollback flow through the
existing min/max, box geometry, clipping, hit-test, display-list, raster, and
PNG capture owners. Box-model declarations, overflow priority, other
properties, dependencies, defaults, multiple origins, and browser-wide CSS
sizing conformance remain outside this slice. Scoped check, focused
units/integration, full native integration (228/228), strict Clippy,
warning-denied rustdoc, and formatting pass locally; final issue-level gates
and remote CI remain pending until issue #40 reaches its final validation
boundary.

The completed dependency-ordered [native-engine-191](tasks/native-engine-191.md)
slice is implemented at `43e5f8c2`. It extends bounded author-origin
`!important` priority to the normal-only physical box-model declarations:
`box-sizing`, physical padding, and physical margin shorthand/longhand edges.
Important-over-normal ordering, reversed named-layer priority, inline
precedence in the unlayered important bucket, per-edge source order,
invalid-later preservation, `auto` margin provenance, and `revert-layer`
rollback flow through the existing content-box/border-box, normal-flow/flex,
overflow, layout, display-list, raster, capture, point-hit, and
semantic/source-order owners. Logical edges, overflow priority, other
properties, and browser-wide CSS conformance remain outside this slice. The
scoped check, focused units/integration, full native integration (229/229),
strict Clippy, warning-denied rustdoc, and formatting pass locally; final
issue-level gates and remote CI remain pending until issue #40 reaches its
final validation boundary.

The completed dependency-ordered [native-engine-192](tasks/native-engine-192.md)
slice is implemented at `fc2c461e` (design `7e693b79`). It extends bounded
author-origin `!important` priority to the normal-only `overflow`, `overflow-x`,
and `overflow-y` declarations through private doubled x/y candidate streams
and shorthand/x/y importance bits. Important-over-normal ordering, reversed
named-layer priority, inline precedence in the unlayered important bucket,
invalid-later preservation, independent axis projection, and
`revert-layer !important` rollback flow through the existing clip, root-overflow,
layout, display-list, raster, capture, point-hit, and semantic/source-order
owners. Nested scrolling, visible/auto/scroll used-value parity, logical writing
modes, multiple origins, and browser-wide CSS overflow conformance remain
outside this slice. Scoped check, focused units/integration, full native
integration (230/230), strict Clippy, warning-denied rustdoc, and formatting
pass locally; final static, paired-crate, package, workspace, security/fuzz,
cleanup, issue-level, and remote-CI gates remain pending until issue #40 reaches
its final validation boundary.

The completed dependency-ordered [native-engine-193](tasks/native-engine-193.md)
slice is implemented at `3e78c246` (design `03bcf403`). It adds bounded
horizontal-tb logical `padding-block`, `padding-inline`, `margin-block`, and
`margin-inline` shorthands plus their block/inline start/end longhands. Resolved
`ltr`/`rtl` direction projects those declarations into the existing physical
per-edge candidate streams while preserving important-over-normal ordering,
reversed named-layer priority, inline-important precedence, invalid-later
behavior, same-rule physical/logical source order, `auto` margin provenance,
and `revert-layer` rollback through the existing geometry, normal-flow/flex,
overflow, display-list, raster, capture, point-hit, and semantic/source-order
owners. Logical `box-sizing`, vertical writing modes, percentages, negative
lengths, margin collapsing, positioning, CSS-wide reset keywords, and
browser-wide conformance remain outside this slice. Scoped check, focused
units/integration, full native integration (231/231), strict Clippy,
warning-denied rustdoc, and formatting pass locally; final static, paired-crate,
package, workspace, security/fuzz, cleanup, issue-level, and remote-CI gates
remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-194](tasks/native-engine-194.md)
slice is implemented at `5477fb79` (design `e643a64c`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to `box-sizing`, physical padding/margin shorthands and longhands, and the
horizontal-tb logical padding/margin family. These forms normalize to the
existing content-box and zero-edge fallbacks while preserving separate
`revert-layer` rollback, important-over-normal ordering, reversed named-layer
priority, inline-important precedence, invalid-later behavior, ltr/rtl
projection, and the existing geometry, normal-flow/flex, overflow, display-list,
raster, capture, point-hit, and semantic/source-order owners. Explicit
`inherit`, percentages, negative lengths, margin collapsing, positioning,
vertical writing modes, additional logical properties, multiple origins,
transitions, animations, and browser-wide CSS-wide conformance remain outside
this slice. Scoped check, focused units/integration, full native integration
(232/232), strict Clippy, warning-denied rustdoc, and formatting pass locally;
final static, paired-crate, package, workspace, security/fuzz, cleanup,
issue-level, and remote-CI gates remain pending until issue #40 reaches its
final validation boundary.

The completed dependency-ordered [native-engine-195](tasks/native-engine-195.md)
slice is implemented at `0caad64b` (design `a61b9c5f`). It adds bounded
standalone case-insensitive `inherit` to `box-sizing`, physical
padding/margin shorthands and longhands, and the supported horizontal-tb
logical padding/margin family. Physical values copy the parent's effective
edges, box-sizing, and private margin `auto` provenance; logical values read
the parent side in its resolved `ltr`/`rtl` direction before projecting into
the child. Root fallbacks, omitted-property non-inheritance, important/source-
order behavior, and `revert-layer` rollback remain bounded by the existing
private cascade. Percentages, negative lengths, margin collapsing, positioning,
vertical writing modes, additional logical properties, multiple origins,
transitions, animations, and browser-wide CSS conformance remain outside this
slice. Scoped check, focused units/integration, full native integration
(233/233), strict Clippy, warning-denied rustdoc, and formatting pass locally;
final static, paired-crate, package, workspace, security/fuzz, cleanup,
issue-level, and remote-CI gates remain pending until issue #40 reaches its
final validation boundary.

The completed dependency-ordered [native-engine-196](tasks/native-engine-196.md)
slice is implemented at `fd6ee415` (design `9231d17f`). It adds bounded
standalone case-insensitive `inherit` to `width`, `height`, `min-width`,
`max-width`, `min-height`, and `max-height`, copying the parent's computed
optional pixel value through the existing private DOM style walk. Explicit
inheritance can carry a parent `None`/auto fallback, while omitted dimensions
remain local and do not inherit. Existing important/source-order, invalid-later,
`revert-layer`, min/max, content-box/border-box, normal-flow/flex, display-list,
raster, PNG-capture, point-hit, and semantic/source-order owners remain bounded.
Percentages, negative lengths, intrinsic sizing, aspect ratio, margin collapsing,
positioning, vertical writing modes, additional origins, transitions, animations,
and browser-wide sizing conformance remain outside this slice. Scoped check,
focused unit/integration, full native integration (234/234), strict Clippy,
warning-denied rustdoc, formatting, static documentation truth/coverage/depth,
feature parity, TUI shortcut, and version-sync gates pass locally; paired-crate,
package, workspace, security/fuzz, cleanup, issue-level, and remote-CI gates
remain pending until issue #40 reaches its final validation boundary.

The completed dependency-ordered [native-engine-197](tasks/native-engine-197.md)
slice is implemented at `a0e102b5` (design `7d71a50c`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to `width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`.
Winning reset candidates resolve to the existing optional `None`/auto fallback
without falling through; `revert-layer` remains the separate lower-layer
rollback candidate. Omission, explicit `inherit`, important/source-order,
invalid-later, min/max, content-box/border-box, normal-flow/flex, display-list,
raster, PNG-capture, point-hit, and semantic/source-order owners remain bounded.
Percentages, negative lengths, intrinsic sizing, aspect ratio, margin collapsing,
positioning, vertical writing modes, additional origins, transitions, animations,
and browser-wide sizing conformance remain outside this slice. Scoped check,
focused unit/integration, full native integration (235/235), strict Clippy,
warning-denied rustdoc, formatting, and static documentation gates pass locally:
release truth reports 611 Markdown documents (83 current, 57 previous-version
hits, 714 semantic audit hits, 0 current-claim failures); coverage reports 611
Markdown files, 345 full-product MCP tools (100 browser-only), 17 examples, and
22 public modules; depth reports 93 current guides and 19 substantive contracts;
feature parity reports 14 capabilities across 4 targets; TUI reports 15
implementation help keys and 63 documentation markers; version sync reports
0.3.14. Paired-crate, package, workspace, security/fuzz, cleanup, issue-level,
and remote-CI gates remain pending until issue #40 reaches its final validation
boundary.

The completed dependency-ordered [native-engine-198](tasks/native-engine-198.md)
slice is implemented at `95a988d0` (design `927cccca`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to inherited `white-space`, positive-pixel `line-height`,
`text-transform`, `font-weight`, `font-style`, `word-break`, `vertical-align`,
`word-spacing`, and `letter-spacing`. Parent/root fallbacks, terminal reset
behavior, invalid-later preservation, existing cascade priority, and
`revert-layer` distinction are covered through the existing computed-style,
layout, display-list, raster, PNG, point-hit, semantic, and diagnostic owners;
the private declaration model and two-crate boundary remain unchanged.
Unit, public integration, full native integration (236/236), full feature
library (1019/1019 with one ignored test and an explicit 8 MiB test-thread
stack), strict Clippy, warning-denied rustdoc, locked scoped check, and
formatting passed locally. Static documentation truth passed with 612 Markdown
documents (83 current, 57 previous-version hits, 715 semantic audit hits, and
0 current-claim failures); coverage, depth, parity, TUI, and version-sync
passed with 612 Markdown files, 345 full-product MCP tools (100 browser-only),
17 examples, 22 public modules, 93 current guides, 19 substantive contracts,
14 capabilities across 4 targets, 15 implementation help keys, 63
documentation markers, and version `0.3.14`. The default-stack library run
still reproduces the pre-existing large-Clap parser test overflow. At that
preceding checkpoint, paired-crate, package, workspace, security/fuzz, static
documentation, cleanup, issue-level, and remote-CI gates remained pending;
the current task evidence follows below.

The completed dependency-ordered [native-engine-199](tasks/native-engine-199.md)
slice is implemented at `4771f352` (design `4b02b41b`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to inherited `text-align`, `text-align-last`, `text-justify`, and
`direction`, retaining `revert-layer` as the named-layer rollback. Parent/root
fallbacks, terminal reset behavior, invalid-later preservation, logical
`ltr`/`rtl` projection, and the existing layout, display-list, raster, PNG,
point-hit, semantic, and diagnostic owners remain bounded. Focused
cascade/parser/integration coverage and full native integration (237/237),
the feature library (1,020 passed, 1 ignored), paired binaries, strict
Clippy, warning-denied rustdoc, workspace all-target/all-feature tests,
doctests, fuzz, package/install, security, static documentation, and formatting
gates pass locally. Direct registry-backed verification of the dev archive is
blocked by the immutable public `glass-browser 0.3.14` API surface, while the
canonical local patched/no-verify route and clean-install transition gate
pass. Exact temporary-target cleanup and issue synchronization remain; remote
CI, push, release, tag, and registry publication are not claimed.

The completed dependency-ordered [native-engine-200](tasks/native-engine-200.md)
slice is implemented at `62525ec4` (design `9eafebc9`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to inherited `text-decoration-style`, retaining `revert-layer` as the
named-layer rollback. Parent/root fallback, `solid` initial behavior,
terminal reset semantics, invalid-later preservation, and the existing finite
decoration styles remain bounded; the existing text display-list, fixed-cell
raster, PNG, and diagnostic owners are reused unchanged. Full native
integration (238/238), the feature library (1,020 passed, 1 ignored), paired
binaries, strict Clippy, warning-denied rustdoc, workspace all-target/all-
feature tests and doctests, fuzz, package, security, and formatting gates pass
locally. Direct registry-backed
dev verification remains blocked by the immutable public `glass-browser
0.3.14` API surface; the documented local patched/no-verify route passes.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-201](tasks/native-engine-201.md)
slice is implemented at `ee7dae83` (design `d9a763db`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to inherited `text-decoration-thickness`, retaining the finite
`1px|2px|3px|4px` values and `revert-layer` rollback. Parent/root fallback,
`1px` initial behavior, terminal reset semantics, invalid-later preservation,
shared decoration geometry, and the two-crate boundary remain bounded. Full
native integration (239/239), the feature library (1,021 passed, 1 ignored),
paired binaries, strict Clippy, warning-denied rustdoc, workspace
all-target/all-feature tests and doctests, fuzz, package, security, and
formatting gates pass locally. Direct registry-backed dev verification remains
blocked by the immutable public `glass-browser 0.3.14` API surface; the
documented local patched/no-verify route passes. Remote CI, push, release, tag,
and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-202](tasks/native-engine-202.md)
slice is implemented at `65e3a76d` (design `d435032c`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to inherited `text-decoration-skip-ink`, retaining finite `auto|none`
paint behavior and `revert-layer` rollback. Parent/root fallback,
terminal-reset semantics, invalid-later preservation, same-run glyph
intersection, display-list, fixed-cell raster, diagnostics, and the two-crate
boundary remain bounded. Full native integration (240/240), the feature
library (1,022 passed, 1 ignored), workspace all-target/all-feature tests,
doctests, paired binaries, strict Clippy, warning-denied rustdoc, fuzz,
package, security, and formatting gates pass locally. Remote CI, push, release,
tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-203](tasks/native-engine-203.md)
slice is implemented at `73c00616` (design `deeedd19`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to the inherited `text-decoration-line` three-bit owner and its
existing bounded `text-decoration` shorthand route, retaining finite line sets
and `revert-layer` rollback. Parent/root fallback, `none` initial behavior,
terminal-reset semantics, invalid-later preservation, shorthand/longhand
routing, display-list, fixed-cell raster, diagnostics, and the two-crate
boundary remain bounded. Full native integration (241/241) and the feature
library (1,023 passed, 1 ignored) pass locally. Remote CI, push, release, tag,
and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-204](tasks/native-engine-204.md)
slice is implemented at `c82773e2` (design `9473832f`). It adds bounded
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` to inherited signed `text-underline-offset`, retaining finite
`-4px..=4px` values and `revert-layer` rollback. Parent/root fallback,
zero-pixel initial behavior, terminal-reset semantics, invalid-later
preservation, important/source order, underline-only movement,
overline/line-through preservation, display-list, fixed-cell raster, and
diagnostics remain bounded. Full native integration (242/242) and the feature
library (1,024 passed, 1 ignored) pass locally. Remote CI, push, release, tag,
and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-205](tasks/native-engine-205.md)
slice is implemented at `46128c2e` (design `8852adee`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `gap`, `row-gap`, and `column-gap`, retaining finite non-negative
pixel values and `revert-layer` rollback. Zero-gap reset fallback,
shorthand/longhand axis projection, important/source order, invalid-later
preservation, flex placement, display-list, raster/PNG, hit testing, and
diagnostics remain bounded; parent-gap propagation, `inherit`, percentages,
fractional/intrinsic values, and generic CSS-wide machinery remain outside the
contract. Full native integration (243/243) and the feature library (1,025
passed, 1 ignored) pass locally. Remote CI, push, release, tag, and registry
publication remain unclaimed.

The completed dependency-ordered [native-engine-206](tasks/native-engine-206.md)
slice is implemented at `b7bd9ace` (design `82c31c9a`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `text-indent` and `text-overflow`, retaining finite non-negative
fixed-pixel indentation, `clip|ellipsis`, and `revert-layer` rollback. Reset
forms resolve to the existing `0px` and `clip` fallbacks respectively while
invalid-later preservation, important/source order, first-line layout,
eligible truncation, display-list, raster/PNG, point-hit, diagnostics, and the
two-crate boundary remain bounded. `inherit`, parent propagation,
negative/fractional or percentage indentation, and browser-wide overflow
conformance remain outside the contract. Full native integration (244/244) and
the feature library (1,026 passed, 1 ignored) pass locally. Remote CI, push,
release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-207](tasks/native-engine-207.md)
slice is implemented at `1a43c150` (design `6d98fb3f`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `flex`, `flex-grow`, `flex-shrink`, and `flex-basis`, retaining finite
shorthand expansion, non-negative fixed-pixel basis values, `auto`, and
`revert-layer` rollback. Reset forms resolve through the existing private
component streams to the finite `0 1 auto` initial tuple while important/source
order, invalid-later preservation, flex placement, display-list, raster/PNG,
point-hit, diagnostics, and the two-crate boundary remain bounded. `inherit`,
percentages, negative/fractional/intrinsic basis values, direction/wrap/
alignment/order, and generic CSS-wide machinery remain outside the contract.
Full native integration (245/245) and the feature library (1,027 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed.

The completed dependency-ordered [native-engine-208](tasks/native-engine-208.md)
slice is implemented at `fcadc99c` (design `e91a358b`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `flex-direction`, `flex-wrap`, and `flex-flow`, retaining finite
row/column direction values, finite wrap modes, shorthand component projection,
and `revert-layer` rollback. Reset forms resolve through the existing private
component streams to `row`/`nowrap` while important/source order, invalid-later
preservation, row/column mapping, wrapping, display-list, raster/PNG, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(246/246) and the feature library (1,029 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-209](tasks/native-engine-209.md)
slice is implemented at `0bfbc8f9` (design `89373bb1`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local flex-item `order`, retaining the signed finite range and
`revert-layer` rollback. Reset forms resolve through the existing local order
resolver to finite `0` while important/source order, invalid-later
preservation, stable visual/source order, flex sizing, display-list, raster/
PNG, point-hit, diagnostics, and the two-crate boundary remain bounded. Full
native integration (247/247) and the feature library (1,030 passed, 1 ignored)
pass locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-210](tasks/native-engine-210.md)
slice is implemented at `e9ac0b1c` (design `e44ce4ec`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `justify-content`, retaining finite distribution values and
`revert-layer` rollback. Reset forms reuse the existing bounded `flex-start`
fallback while important/source order, invalid-later preservation, free-space
distribution, row/column mapping, flex sizing, display-list, raster/PNG,
point-hit, diagnostics, and the two-crate boundary remain bounded. Full native
integration (248/248) and the feature library (1,031 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-211](tasks/native-engine-211.md)
slice is implemented at `29e23b4a` (design `c5c36b40`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `align-items`, retaining finite cross-axis values and `revert-layer`
rollback. Reset forms reuse the existing bounded `flex-start` fallback while
important/source order, invalid-later preservation, cross-axis placement,
`align-self` overrides, flex sizing, display-list, raster/PNG, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(249/249) and the feature library (1,032 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-212](tasks/native-engine-212.md)
slice is implemented at `0a624642` (design `68f42ebe`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `align-self`, retaining finite item values and `revert-layer` rollback.
Reset forms reuse the existing bounded `auto` fallback and continue through
parent `align-items`; important/source order, invalid-later preservation,
complete-subtree movement, flex sizing, display-list, raster/PNG, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(250/250) and the feature library (1,033 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-213](tasks/native-engine-213.md)
slice is implemented at `337da7c5` (design `e82eea50`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to local `align-content`, retaining finite wrapped-line distribution values and
`revert-layer` rollback. Reset forms reuse the existing bounded `flex-start`
fallback while terminal reset, invalid-later preservation, important/source
order, wrapped-line distribution, item alignment, flex sizing, display-list,
raster/PNG, point-hit, diagnostics, and the two-crate boundary remain bounded.
Full native integration (251/251) and the feature library (1,034 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed.

The completed dependency-ordered [native-engine-214](tasks/native-engine-214.md)
slice is implemented at `24ae15a7` (design `9f26239d`). It adds bounded
standalone case-insensitive `initial`, `unset`, and one-author-origin `revert`
to the local `place-content` shorthand projection, retaining finite one-/two-
value expansion and `revert-layer` rollback. Reset forms project through the
existing bounded `flex-start` fallbacks for both `align-content` and
`justify-content`; terminal reset, invalid-later preservation, important/source
order, wrapped-line distribution, main-axis placement, item alignment, flex
sizing, display-list, raster/PNG, point-hit, diagnostics, and the two-crate
boundary remain bounded. Full native integration (252/252) and the feature
library (1,035 passed, 1 ignored) pass locally. Remote CI, push, release, tag,
and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-215](tasks/native-engine-215.md)
slice is implemented at `a66f474b` (design `0cd2040d`). It adds bounded
standalone case-insensitive `align-content: inherit` through the existing
private ancestor-style chain while keeping omitted `align-content`
non-inherited with the bounded `flex-start` fallback. Mixed invalid forms,
source order, wrapped-line placement, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Full native
integration (253/253) and the feature library (1,036 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-216](tasks/native-engine-216.md)
slice is implemented at `c3dc1faf` (design `485c5750`). It adds bounded
standalone case-insensitive `justify-content: inherit` through the existing
private ancestor-style chain while keeping omitted `justify-content`
non-inherited with the bounded `flex-start` fallback. Mixed invalid forms,
source order, main-axis placement, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Full native
integration (254/254) and the feature library (1,037 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-217](tasks/native-engine-217.md)
slice is implemented at `feb6b607` (design `5ee75fcb`). It adds bounded
standalone case-insensitive `align-items: inherit` through the existing private
ancestor-style chain while keeping omitted `align-items` non-inherited with the
bounded `flex-start` fallback. Mixed invalid forms, source order, cross-axis
placement, `align-self` overrides, display-list, raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary remain bounded. Full native
integration (255/255) and the feature library (1,038 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-218](tasks/native-engine-218.md)
slice is implemented at `52adc7d1` (design `ca9afd5f`). It adds bounded
standalone case-insensitive `align-self: inherit` through the existing private
ancestor-style chain while keeping omitted `align-self` local `auto`; explicit
`auto` continues to delegate to the containing flex parent's `align-items`.
Mixed invalid forms, source order, cross-axis placement, complete-subtree
movement, display-list, raster/PNG, point-hit, semantics, diagnostics, and the
two-crate boundary remain bounded. Full native integration (256/256) and the
feature library (1,039 passed, 1 ignored) pass locally. Remote CI, push,
release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-219](tasks/native-engine-219.md)
slice is implemented at `23703765` (design `0b9e784b`). It adds bounded
standalone case-insensitive `place-content: inherit` through the existing
`align-content` and `justify-content` inheritance owners, copying both
computed parent components only when explicitly authored while keeping omitted
`place-content` local. Mixed invalid forms, source order, important priority,
finite shorthand expansion, `revert-layer`, wrapped-line distribution,
main-axis placement, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(257/257) and the feature library (1,040 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-220](tasks/native-engine-220.md)
slice is implemented at `1f64f232` (design `8ffe1f17`). It adds bounded
standalone case-insensitive `flex-direction: inherit` through the existing
private parent-style chain, copying the computed parent direction while keeping
omitted `flex-direction` local with the bounded `row` fallback. Mixed invalid
forms, source order, important priority, finite `flex-flow`/longhand component
precedence, `revert-layer`, row/column mapping, wrapping eligibility, gap and
margin mapping, flex sizing, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(258/258) and the feature library (1,041 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-221](tasks/native-engine-221.md)
slice is implemented at `ebeb8443` (design `2925cc3a`). It adds bounded
standalone case-insensitive `flex-wrap: inherit` through the existing private
parent-style chain, copying the computed parent wrap mode while keeping
omitted `flex-wrap` local with the bounded `nowrap` fallback. Mixed invalid
forms, source order, important priority, finite `flex-flow`/longhand component
precedence, `revert-layer`, row/column wrapping eligibility, line formation and
reverse stacking, line sizing, gap and margin mapping, flex sizing, display-list,
raster/PNG, point-hit, semantics, diagnostics, and the two-crate boundary
remain bounded. Full native integration (259/259) and the feature library
(1,042 passed, 1 ignored) pass locally. Remote CI, push, release, tag, and
registry publication remain unclaimed.

The completed dependency-ordered [native-engine-222](tasks/native-engine-222.md)
slice is implemented at `87465d23` (design `cae84efb`). It adds bounded
standalone case-insensitive `flex-flow: inherit` by projecting to the existing
private `flex-direction` and `flex-wrap` inheritance owners, copying both
computed parent components while keeping omitted `flex-flow` local with the
bounded `row`/`nowrap` fallbacks. Mixed invalid forms, source order, important
priority, finite one-/two-value expansion, longhand/component precedence,
`revert-layer`, row/column mapping, wrapping eligibility, line formation and
reverse stacking, gap and margin mapping, flex sizing, display-list, raster/PNG,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Full native integration (260/260) and the feature library (1,043 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed.

The completed dependency-ordered [native-engine-223](tasks/native-engine-223.md)
slice is implemented at `c94b6006` (design `f97ac088`). It adds bounded
standalone case-insensitive `flex: inherit` by projecting to the existing
private `flex-grow`, `flex-shrink`, and `flex-basis` inheritance owners,
copying all three computed parent components while keeping omitted `flex` local
with the bounded `0 1 auto` fallbacks. Mixed invalid forms, source order,
important priority, finite shorthand expansion, longhand/component precedence,
`revert-layer`, row/column and wrapped sizing, display-list, raster/PNG,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Full native integration (261/261) and the feature library (1,044 passed, 1
ignored) pass locally. Remote CI, push, release, tag, and registry publication
remain unclaimed.

The completed dependency-ordered [native-engine-224](tasks/native-engine-224.md)
slice is implemented at `bc8de568` (design `4402c331`). It adds bounded
standalone case-insensitive `flex-grow: inherit` through the existing private
grow component and ancestor-style chain, copying the computed parent grow
value while keeping omitted `flex-grow` local with the bounded `0` fallback.
Mixed invalid forms, source order, important priority, finite
shorthand/longhand precedence, reset semantics, `revert-layer`, row/column
and wrapped sizing, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native
integration (262/262) and the feature library (1,044 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-225](tasks/native-engine-225.md)
slice is implemented at `b4f465db` (design `7064801b`). It adds bounded
standalone case-insensitive `flex-shrink: inherit` through the existing private
shrink component and ancestor-style chain, copying the computed parent shrink
value while keeping omitted `flex-shrink` local with the bounded `1` fallback.
Mixed invalid forms, source order, important priority, finite
shorthand/longhand precedence, reset semantics, `revert-layer`, row/column
and wrapped sizing, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native
integration (263/263) and the feature library (1,044 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-226](tasks/native-engine-226.md)
slice is implemented at `4ee2e1ed` (design `c52398fd`). It adds bounded
standalone case-insensitive `flex-basis: inherit` through the existing private
basis component and ancestor-style chain, copying the computed parent basis
value—including the bounded `auto` fallback—while keeping omitted
`flex-basis` local. Mixed invalid forms, source order, important priority,
finite shorthand/longhand precedence, reset semantics, `revert-layer`,
row/column and wrapped sizing, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native
integration (264/264) and the feature library (1,044 passed, 1 ignored) pass
locally. Remote CI, push, release, tag, and registry publication remain
unclaimed.

The completed dependency-ordered [native-engine-227](tasks/native-engine-227.md)
slice is implemented at `6dde525a` (design `330680b6`), with compatibility
coverage retained at `1e7663c1`. It adds bounded standalone case-insensitive
`order: inherit` through the existing private parent-style chain, copying the
computed parent order while keeping omitted `order` local with the bounded `0`
fallback. Visual `(order, source_index)` sorting remains separate from
semantic/source order; mixed invalid forms, source order, important priority,
reset semantics, `revert-layer`, display-list, raster/PNG, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(265/265) and the feature library (1,044 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-228](tasks/native-engine-228.md)
slice is implemented at `978ae3b5` (design `8241ef68`), with compatibility
coverage retained at `b645585b`. It adds bounded standalone case-insensitive
`gap: inherit` through the existing private parent-style chain, copying the
computed parent row and column gap components while keeping omitted `gap` local
with the bounded `0` fallback. Direct `row-gap: inherit` and
`column-gap: inherit` remain unsupported by design; mixed invalid forms,
important priority, shorthand/longhand precedence, reset semantics,
`revert-layer`, wrapped/column placement, display-list, raster/PNG, point-hit,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(266/266) and the feature library (1,045 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-229](tasks/native-engine-229.md)
slice is implemented at `6c4116e4` (design `0e2916c9`). It adds bounded
standalone case-insensitive `row-gap: inherit` and `column-gap: inherit`
through the existing private parent-style chain, copying the corresponding
computed parent gap component only when explicitly authored while omitted
longhands remain local with the bounded `0` fallback. Independent-axis
cascade, layer/importance/source-order precedence, shorthand/longhand
interaction, reset and `revert-layer` behavior, mixed-invalid preservation,
wrapped/column placement, display-list, raster/PNG, point-hit, semantics,
diagnostics, and the two-crate boundary remain bounded. Full native integration
(267/267) and the feature library (1,046 passed, 1 ignored) pass locally.
Remote CI, push, release, tag, and registry publication remain unclaimed.

The completed dependency-ordered [native-engine-230](tasks/native-engine-230.md)
slice is implemented at `5d214fee` (design `3782eb4b`). It adds bounded
standalone case-insensitive `text-indent: inherit` through the existing private
parent-style chain, copying the computed parent first-line indent only when
explicitly authored while omitted `text-indent` remains local with the bounded
`0` fallback. Finite values, CSS-wide resets, `revert-layer`, mixed-invalid
preservation, source-order/important precedence, first-line wrapping, text
fragments, display-list, raster/PNG, point-hit, semantics, diagnostics, and the
two-crate boundary remain bounded. Focused parser/cascade coverage (3 tests),
public inheritance/artifact integration (1 test), full-native integration
(268/268), the feature library (1,047 passed, 1 ignored), and the locked
scoped check pass locally. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, and promotion remain unclaimed.

The completed dependency-ordered [native-engine-231](tasks/native-engine-231.md)
slice is implemented at `9a9ef2c7` (design `57e37dfe`). It adds bounded
standalone case-insensitive `text-overflow: inherit` through the existing
private parent-style chain, copying the computed parent `clip|ellipsis` value
only when explicitly authored while omitted `text-overflow` remains local with
the bounded `clip` fallback. Finite values, CSS-wide resets, `revert-layer`,
mixed-invalid preservation, source-order/important precedence, eligible
clipped-nowrap truncation, text fragments, display-list, raster/PNG, overflow,
point-hit, semantics, diagnostics, and the two-crate boundary remain bounded.
Focused parser/cascade coverage (3 tests), public truncation/artifact
integration (1 test), full-native integration (269/269), the feature library
(1,048 passed, 1 ignored), and the locked scoped check pass locally. Remote CI,
push, release, tag, registry publication, browser-parity, security-boundary,
and promotion remain unclaimed.

The completed dependency-ordered [native-engine-232](tasks/native-engine-232.md)
slice is implemented at `1a7df31b` (design `0a617b8c`, contract clarification
`2e8bcf9a`). It adds bounded standalone case-insensitive `overflow: inherit`,
`overflow-x: inherit`, and `overflow-y: inherit` through the existing private
parent-style chain, copying the parent's effective clip/no-clip axis
projections only when explicitly authored while omitted overflow remains local
with the visible/no-clip fallback. It preserves shorthand/longhand and
important precedence, mixed-invalid preservation, axis-specific clip/scroll
projection, display-list, fixed-cell raster/PNG, point-hit, semantic/source
order, diagnostics, and the two-crate boundary. The focused batch passed all 9
library-target tests and 19 matching native integration tests, and the scoped
native-feature check plus formatting/diff checks passed locally. Full issue-level
gates, remote CI, push, release, tag, registry publication, browser-parity,
security-boundary, and promotion remain unclaimed.

The completed dependency-ordered [native-engine-233](tasks/native-engine-233.md)
slice is implemented at `7b31b72e` (design `70f4f924`). It adds bounded
standalone case-insensitive `overflow: initial`, `overflow: unset`, and
one-author-origin `overflow: revert` reset forms for the shorthand and
longhands, resolving each affected axis to the visible/no-clip fallback while
preserving explicit `inherit`, named-layer `revert-layer`, priority,
axis-specific clipping, root scroll projection, display-list, fixed-cell
raster/PNG, point-hit, semantic/source order, diagnostics, and the two-crate
boundary. The focused reset batch passed 1 library-target test and 1 matching
native integration test; the scoped check plus formatting/diff checks passed
locally. Full issue-level gates, remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion remain
unclaimed.

The completed dependency-ordered [native-engine-234](tasks/native-engine-234.md)
slice is implemented at `8a96f56b` (design `b2119e5f`). It accepts bounded
standalone case-insensitive `overflow: visible|auto|scroll`,
`overflow-x: visible|auto|scroll`, and `overflow-y: visible|auto|scroll`
values as the existing visible/no-clip projection, without nested scroll
containers or scrollbar artifacts. It preserves explicit `inherit`, CSS-wide
reset forms, named-layer `revert-layer`, priority, axis-specific clipping,
root scroll projection, display-list, fixed-cell raster/PNG, point-hit,
semantics, diagnostics, and the two-crate boundary. The focused no-clip pair
passed 1 library-target test and 1 native integration test; the broader
overflow-filtered regression batch passed 11 library-target tests and 21
matching native integration tests; and the scoped check plus formatting/diff
checks passed locally. Full issue-level gates, remote CI, push, release, tag,
registry publication, browser-parity, security-boundary, and promotion remain
unclaimed.

The dependency-ordered [native-engine-121](tasks/native-engine-121.md)
implementation is complete at `f361415a`. It accepts explicit case-insensitive
`text-decoration-skip-spaces: revert` through a distinct private
declaration-only value and resolves it at the current one-author-origin
inherited fallback boundary, keeping the public finite paint enum and all
artifact consumers unchanged. `revert-layer`, cascade layers, multiple style
origins, general CSS-wide keyword machinery, layout/raster changes, new
dependencies, default-feature changes, and crate-boundary changes remain
outside this slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

The dependency-ordered [native-engine-122](tasks/native-engine-122.md)
implementation is complete at `1291fc2c`, with the strict-Clippy parser-context
follow-up at `efb5bdfc`. It adds bounded top-level named cascade layers,
private first-appearance priority ahead of selector specificity, a 15-layer
limit, and `text-decoration-skip-spaces: revert-layer` rollback through lower
candidates and the existing inherited/root fallback while preserving the
unlayered/inline bucket above named layers. Layer statements, anonymous/comma/
nested layers, multiple origins, and general CSS-wide keyword machinery remain
outside the slice. Focused, full-native, two-crate, strict, package, fuzz,
security, formatting, and static local certification passed; exact evidence
and cleanup are recorded in the task. Remote CI remains pending because the
checkout is local-only.

## Historical plan: Glass v0.3.6 issue #36

Status: Historical/superseded — this 0.3.6 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue [#36](https://github.com/wanazhar/glass/issues/36) is the authoritative
urgent product-repair contract. The 35-pillar baseline, locked architecture,
dependency order, scenarios A-J, and 15 release gates are mapped in
[the v0.3.6 delivery analysis](analysis/release-036.md). The serial tasks are:

1. [correctness-036-001](tasks/correctness-036-001.md) — lifecycle, operations,
   verification, Pi setup, and semantic documentation truth;
2. [browser-workspace-036-002](tasks/browser-workspace-036-002.md) — one shared
   browser controller/view and standalone/embedded parity;
3. [product-ux-036-003](tasks/product-ux-036-003.md) — Agent, Code, App,
   Terminal, Tasks, Git, Debug and contextual desktop interaction;
4. [mobile-onboarding-036-004](tasks/mobile-onboarding-036-004.md) — intentional
   phone navigation, onboarding, and searchable palette;
5. [certification-036-005](tasks/certification-036-005.md) — integrated, PTY,
   remote-boundary, package, exact-tag, publication, and release evidence.

Each completed checkpoint receives a focused conventional commit before the
next task starts.

## Historical plan: Glass v0.3.5 issue #35

Status: Historical/superseded — this 0.3.5 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue [#35](https://github.com/wanazhar/glass/issues/35) is the authoritative
sixteen-pillar trust and runtime-convergence contract. The audited baseline,
dependency order, twelve release gates, scenarios A-I, forbidden outcomes, and
disk-aware validation policy are in [the v0.3.5 delivery analysis](analysis/release-035.md).
The first release-blocking checkpoint is the
[workspace trust boundary](tasks/security-035-001.md).
The [native Pi SDK boundary](tasks/runtime-035-002.md) and
[product-boundary deletion](tasks/product-boundary-035-013.md) are complete in
the source candidate.
The [autonomous task and workspace actor checkpoint](tasks/scheduler-035-003.md)
is also complete locally and removes the global daemon workspace lock.
The [native transport checkpoint](tasks/platform-035-004.md) implements Windows
named pipes; only a remote native Windows run can certify that platform.
The [automatic experiment evidence checkpoint](tasks/experiments-035-005.md)
adds measured provenance and trusted deterministic ranking.

Delivery order:

1. workspace trust and customization authority;
2. native Pi SDK and completed package ownership migration;
3. autonomous verified task DAGs;
4. per-workspace daemon actors and native Windows IPC;
5. automatic experiments, DAP breadth, kernel bindings, graph/replay;
6. TUI parity, stress scenarios, documentation, packaging, and release gates.

Each completed checkpoint receives a focused local conventional commit before
the next checkpoint starts.

## Historical plan: Glass v0.3.4 issue #34

Status: Historical — released and publicly verified on 2026-08-10.

Issue [#34](https://github.com/wanazhar/glass/issues/34) is the authoritative
18-pillar full-agentic-development-suite contract. The baseline inventory,
dependency order, checkpoint boundaries, forbidden outcomes, and release gate
evidence map are in [the v0.3.4 delivery analysis](analysis/release-034.md).
The live audit is in the [v0.3.4 gate review](reviews/release-034-gates.md).

Delivery order:

1. ownership and TUI boundary decomposition;
2. native Pi sessions, governed resident tools, and agent scheduling;
3. shared LSP, real DAP, Git, tests, and persistent kernels;
4. durable workspace ownership, experiments, graph, and replay;
5. agent-native desktop/mobile surfaces and integrated browser/workflow tools;
6. synchronized 0.3.4 packaging, full-system demonstrations, and release gates.

Every completed delivery checkpoint is committed locally with a focused
conventional commit before the next checkpoint begins.

## Historical plan: Glass 0.3.3 documentation depth

Status: Historical — completed and verified locally as documentation-only, with
no push, tag, publication, release, or issue mutation.

The [depth audit](analysis/documentation-depth-035.md) accounts for every
public documentation surface and replaces name-presence checks with workflow,
state, limit, failure, recovery, and exact nested-command contracts. The
implementation record is
[documentation-depth-035-001](tasks/documentation-depth-035-001.md).

## Historical plan: Glass v0.3.3 issue #33

Status: Historical/superseded — this 0.3.3 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue #33 and its authoritative amendment define 15 integration pillars, 53
mandatory release checkboxes, scenarios A–K, full-suite command exposure and
zero-exit browser recovery. The complete defect baseline, integration chains,
TUI inspiration decisions and evidence matrix are in
[analysis/release-033.md](analysis/release-033.md); the auditable 53/53 mapping
is in [reviews/release-033-gates.md](reviews/release-033-gates.md).

Delivery order:

1. [presentation-033-001](tasks/presentation-033-001.md) — independent
   connection dimensions, policy matrix, observatory and correct local pacing.
2. [tui-recovery-033-002](tasks/tui-recovery-033-002.md) — phone shell,
   command palette, browser controller, recovery sheet and target picker.
3. [remote-agent-033-003](tasks/remote-agent-033-003.md) — secure Remote View
   and live attachment-aware agent context.
4. [runtime-033-004](tasks/runtime-033-004.md) — process trees, project tree,
   persistent LSP and real embedded Neovim proof.
5. [release-033-005](tasks/release-033-005.md) — full-suite binaries,
   cross-platform/tool CI, black-box demonstrations, docs and final validation.

## Historical plan: complete public documentation and docs.rs revamp

Status: Historical — completed and verified locally; no remote mutation.

The [documentation audit](analysis/documentation-revamp-034.md) and
[implementation task](tasks/documentation-034-001.md) cover every current
user, operator, SDK, MCP, TUI, package, rustdoc, example, and release-reference
surface, with generated drift checks against the implementation.

## Historical plan: semantic resource and correctness audit

Status: Historical — completed and verified locally; no remote mutation.

The [resource audit](analysis/semantic-resource-audit.md) and atomic
[implementation task](tasks/semantic-resource-033-002.md) optimize the task
compiler, live binding, agent gateway, and private Pi request boundary while
preserving the completed semantic-core contracts.

## Historical plan: semantic core hardening

Status: Historical — completed and verified locally; no remote mutation.

The [delivery analysis](analysis/semantic-core-hardening.md) and atomic
[implementation task](tasks/semantic-core-033-001.md) cover executable Web IR
evidence, relationship-scoped compilation, revision-bound execution,
capability and state fidelity, continuity, and bounded Local/Pi semantic tools.

## Historical/superseded post-0.1.18 roadmap
Status: Historical/superseded — the roadmap body records delivered 0.2.x work
and later issue tracking; it is not the current source-line plan.

Issue #21 was delivered as a serial workflow-runtime sprint. Issues #25 and
#26 were included in the published 0.2.0 release; their remaining artifact and
cross-platform evidence is tracked by issue #28. The reliability task records
are [scenario contract](tasks/reliability-026-001.md),
[adversarial fixture](tasks/reliability-026-002.md), [certification gate](tasks/reliability-026-003.md),
and [capability/replay work](tasks/reliability-026-004.md). Issue #27 is now
active; its first foundation phase is [stable runtime platform](tasks/platform-027-001.md).

## Historical plan: Glass v0.3.1 issue #31

Status: Historical/superseded — this 0.3.1 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

The authoritative [issue #31](https://github.com/wanazhar/glass/issues/31)
defines four mandatory pillars—semantic memory, multi-surface understanding,
runtime survivability beyond CDP, and the Ratatui-native Browser Workspace—
plus the cross-cutting Glass Experience Layer. The delivery analysis,
integration inventory, and dependency order are in
[analysis/release-031.md](analysis/release-031.md).

Foundation wave:

1. [memory-031-001](tasks/memory-031-001.md) — surface/backend-aware
   knowledge provenance and explainability.
2. [surface-031-001](tasks/surface-031-001.md) — bounded multi-surface
   contract.
3. [backend-031-001](tasks/backend-031-001.md) — transport-neutral Browser
   Capability Interface.
4. [presentation-031-001](tasks/presentation-031-001.md) — bounded latest-frame
   presentation contract.
5. [workspace-031-001](tasks/workspace-031-001.md) — addressable workspace and
   mutation-lease contract.

These tasks are intentionally disjoint and do not edit shared exports or
dispatch files. Integration tasks begin only after committed implementation
and independent review.

## Historical plan: Glass v0.3.2 issue #32

Status: Historical/superseded — this 0.3.2 audit body is retained for issue and
release provenance; the latest published release evidence is 0.3.14. Current-checkout
additions are listed in the current 0.3.14 source section above.

Issue #32 is an architectural epic. The original thin-slice interpretation was
rejected during the issue/comment audit; the current candidate is reviewed
against every pillar, mandatory release gate, visual comment, and packaging
comment in the [delivery evidence matrix](analysis/release-032.md).

Delivery order:

1. [development-032-001](tasks/development-032-001.md) — project detection,
   bounded files/editor, PTY process runtime, events, graph, and diff core.
2. [development-032-002](tasks/development-032-002.md) — shared CLI and MCP
   project-runtime contracts for humans and external agents.
3. [development-032-003](tasks/development-032-003.md) — native TUI project
   surface and embedded harness interaction.
4. [release-032-001](tasks/release-032-001.md) — package boundary, versioned
   documentation, release validation, and local 0.3.2 candidate checkpoint.

The candidate must keep the v0.3.1 browser intelligence contracts intact. Any
capability that cannot provide evidence in this checkout is reported as
experimental or unavailable; it is not presented as a completed framework
integration.

Integration wave (after foundation review):

- [surface-031-002](tasks/surface-031-002.md) — integrate surfaces into Web
  IR extraction.
- [backend-031-002](tasks/backend-031-002.md) — route the CDP implementation
  through the Browser Capability Interface.
- [presentation-031-002](tasks/presentation-031-002.md) — add terminal graphics,
  bounded frame presentation, and semantic fallback.
- [workspace-031-002](tasks/workspace-031-002.md) — connect workspace identity
  to profiles, sessions, memory, and attachments.
- [memory-031-002](tasks/memory-031-002.md) — connect retrieval and provenance
  to compiler advisories.
- [experience-031-001](tasks/experience-031-001.md) — expose the shared
  Experience Layer across CLI, TUI, and MCP.
- [integration-031-001](tasks/integration-031-001.md) — run the integrated
  four-pillar conformance demonstration.

## Historical plan: remote development cockpit

Status: Historical — implemented and verified by direct serial work on the
local 0.3.2 candidate.

The post-issue-32 product enhancements are defined in the
[delivery analysis](analysis/mobile-cockpit.md) and implemented as the atomic
[mobile-cockpit-001](tasks/mobile-cockpit-001.md) task.

## Proposed plan: best-in-class agent browser

Status: Proposed draft — requires owner approval before implementation; it is
not a commitment for the current `0.3.14` source line.

The proposed goal is to make Glass a deterministic, memory-efficient browser
control layer that humans and agents prefer over mature alternatives for local
Chrome automation.

Analysis and scorecard:

- [Best-in-class browser analysis](analysis/best-in-class-browser.md)

### Task order

| Order | Task | Outcome |
|---:|---|---|
| 1 | [quality-007](tasks/quality-007.md) | Task-success and resource scorecard before feature work. |
| 2 | [mcp-008](tasks/mcp-008.md) | Bounded, negotiated, cancellable MCP transport. |
| 3 | [target-009](tasks/target-009.md) | Unique locator resolution and verified hit targets. |
| 4 | [wait-010](tasks/wait-010.md) | Typed explicit wait engine. |
| 5 | [topology-011](tasks/topology-011.md) | Tabs, popups, targets, and frames. |
| 6 | [input-012](tasks/input-012.md) | Complete keyboard, pointer, form, and upload primitives. |
| 7 | [observe-013](tasks/observe-013.md) | Consistent, frame-aware, bounded observations. |
| 8 | [diagnostic-014](tasks/diagnostic-014.md) | Scoped console, network, dialog, and download evidence. |
| 9 | [visual-015](tasks/visual-015.md) | Exact viewport/full-page capture and visual verification. |
| 10 | [policy-016](tasks/policy-016.md) | Enforceable safety profiles and side-effect controls. |
| 11 | [release-017](tasks/release-017.md) | Supply-chain, fuzz, crash, and multi-platform hardening. |
| 12 | [compare-018](tasks/compare-018.md) | Final comparative task-success and efficiency gate. |
| 13 | [observe-019](tasks/observe-019.md) | Event-driven accessibility rejected against pinned Chromium semantics. |

Tasks are developed and independently reviewed in dependency order. A phase
does not advance while correctness or safety gates from an earlier task fail.

### Completed plan: Glass 0.2.2 issue #29

Status: Complete — published on crates.io; follow-up work continues in the 0.2.3 development line

The complete issue outline, dependency map, integration points, and atomic
commit boundaries are recorded in
[analysis/release-029.md](analysis/release-029.md). Phase tasks:

1. [release-029-001](tasks/release-029-001.md) — contract foundations
2. [release-029-002](tasks/release-029-002.md) — installation diagnostics
3. [release-029-003](tasks/release-029-003.md) — bounded agent operations
4. [release-029-004](tasks/release-029-004.md) — state and templates
5. [release-029-005](tasks/release-029-005.md) — maintainability and distribution
6. [release-029-006](tasks/release-029-006.md) — release verification

### Completed plan: Glass Semantic Execution Engine issue #30

Status: Complete — `glass-browser 0.3.0` is published on crates.io and
`v0.3.0` has the matching source-only GitHub Release. The epic exit contract
and release delivery record are complete.

Completed:

- [ir-030-001](tasks/ir-030-001.md) — versioned fixture corpus and static
  baseline inventory.
- [ir-030-002](tasks/ir-030-002.md) — bounded extraction contracts, scopes, and
  resource budgets.
- [ir-030-003](tasks/ir-030-003.md) — evidence quality and coverage metadata.
- [ir-030-004](tasks/ir-030-004.md) — deterministic draft Web IR
  reconciliation.
- [ir-030-005](tasks/ir-030-005.md) — explicit opaque boundary graph entities.
- [ir-030-006](tasks/ir-030-006.md) — bounded region relationship evidence.
- [ir-030-007](tasks/ir-030-007.md) — fixture-derived draft graph expectations.
- [ir-030-008](tasks/ir-030-008.md) — evidence-backed form ownership edges.
- [ir-030-009](tasks/ir-030-009.md) — explicit relationship hints.
- [ir-030-010](tasks/ir-030-010.md) — source-level relationship hint
  validation.
- [ir-030-011](tasks/ir-030-011.md) — validated relationship-hint
  diagnostics.
- [ir-030-012](tasks/ir-030-012.md) — unmatched relationship-hint statuses.
- [ir-030-013](tasks/ir-030-013.md) — emitted and unmatched diagnostic
  expectations.
- [ir-030-014](tasks/ir-030-014.md) — corpus hint-diagnostic expectations.
- [ir-030-015](tasks/ir-030-015.md) — runtime custom-control hints.
- [ir-030-016](tasks/ir-030-016.md) — expanded custom-control hints.
- [ir-030-017](tasks/ir-030-017.md) — deterministic Web IR revision diffs.
- [ir-030-018](tasks/ir-030-018.md) — revision identity continuity.
- [ir-030-019](tasks/ir-030-019.md) — strict Task Protocol v1 contract.
- [ir-030-020](tasks/ir-030-020.md) — deterministic Task Protocol execution plans.
- [ir-030-021](tasks/ir-030-021.md) — typed task.compile protocol boundary.
- [ir-030-022](tasks/ir-030-022.md) — typed compiled-plan response.
- [ir-030-023](tasks/ir-030-023.md) — browser-free MCP compileTask integration.
- [ir-030-024](tasks/ir-030-024.md) — MCP compileTask client documentation.
- [ir-030-025](tasks/ir-030-025.md) — browser-free CLI task compile.
- [ir-030-026](tasks/ir-030-026.md) — typed MCP compileTask errors.
- [ir-030-027](tasks/ir-030-027.md) — compiler explanation mode.
- [ir-030-028](tasks/ir-030-028.md) — compiled-plan guard metadata.
- [ir-030-029](tasks/ir-030-029.md) — browser-free task validation.
- [ir-030-030](tasks/ir-030-030.md) — browser-free MCP task validation.
- [ir-030-031](tasks/ir-030-031.md) — Rust crate-root extraction and Web IR APIs.
- [ir-030-032](tasks/ir-030-032.md) — browser-free Web IR inspect and diff CLI.
- [ir-030-033](tasks/ir-030-033.md) — offline Web IR entity continuity classification.
- [ir-030-034](tasks/ir-030-034.md) — deterministic Web IR canonical JSON CLI output.
- [ir-030-035](tasks/ir-030-035.md) — offline Web IR validation command.
- [ir-030-036](tasks/ir-030-036.md) — browser-free MCP Web IR inspection and
  validation.
- [ir-030-037](tasks/ir-030-037.md) — browser-free MCP Web IR diff and
  continuity classification.
- [ir-030-038](tasks/ir-030-038.md) — canonical Glass protocol operations for
  Web IR revision analysis.
- [ir-030-039](tasks/ir-030-039.md) — canonical Glass protocol operations for
  Web IR inspection and validation.
- [ir-030-040](tasks/ir-030-040.md) — canonical Glass protocol operations for
  Task Protocol validation and compilation.
- [ir-030-041](tasks/ir-030-041.md) — route MCP Task Protocol tools through
  typed canonical dispatch.
- [ir-030-042](tasks/ir-030-042.md) — route MCP Web IR tools through typed
  canonical dispatch.
- [ir-030-043](tasks/ir-030-043.md) — typed canonical protocol response fixture
  coverage.
- [ir-030-044](tasks/ir-030-044.md) — typed canonical preflight error fixture
  coverage.
- [ir-030-045](tasks/ir-030-045.md) — advertise canonical Task and Web IR
  schema versions through capability negotiation.
- [ir-030-046](tasks/ir-030-046.md) — advertise Task Protocol and Web IR
  capability statuses.
- [ir-030-047](tasks/ir-030-047.md) — typed MCP Task validation and compilation
  errors.
- [ir-030-048](tasks/ir-030-048.md) — typed canonical Task compilation
  preflight error coverage.
- [ir-030-049](tasks/ir-030-049.md) — route CLI Task commands through canonical
  protocol helpers.
- [ir-030-050](tasks/ir-030-050.md) — route safe CLI Web IR projections through
  canonical protocol helpers.
- [ir-030-051](tasks/ir-030-051.md) — typed Web IR diff and continuity
  preflight error fixture coverage.
- [ir-030-052](tasks/ir-030-052.md) — expose the bounded canonical Web IR
  diff projection through an explicit offline CLI mode.
- [ir-030-053](tasks/ir-030-053.md) — harden deterministic Task Protocol
  execution-plan safety checks.
- [ir-030-054](tasks/ir-030-054.md) — enforce compatible Web IR revision
  transitions for diffs and continuity.
- [ir-030-055](tasks/ir-030-055.md) — verified form task execution boundary
  (implemented and covered in `0.2.7`).
- [ir-030-056](tasks/ir-030-056.md) — expose verified form task execution
  through CLI and MCP.
- [ir-030-057](tasks/ir-030-057.md) — execute bounded semantic region
  extraction through the guarded Task Protocol runtime.
- [ir-030-058](tasks/ir-030-058.md) — standardize typed task retry guidance
  across guarded execution outcomes.
- [ir-030-059](tasks/ir-030-059.md) — execute revision-guarded navigation
  tasks through CLI, MCP, and Rust.
- [ir-030-060](tasks/ir-030-060.md) — execute revision-guarded semantic tab
  selection within scoped regions.
- [ir-030-061](tasks/ir-030-061.md) — execute guarded inspect, confirm, and
  cancel dialog tasks through CLI, MCP, and Rust.
- [ir-030-062](tasks/ir-030-062.md) — execute bounded revision-guarded
  pagination advances within semantic pagination regions.
- [ir-030-063](tasks/ir-030-063.md) — expose typed pending-dialog details
  through `dialog.inspect` task results.
- [ir-030-064](tasks/ir-030-064.md) — route CLI and MCP browser-backed task
  execution through one canonical Rust dispatcher.
- [ir-030-065](tasks/ir-030-065.md) — execute bounded `collection.extract`
  against uniquely scoped semantic collection regions.
- [ir-030-066](tasks/ir-030-066.md) — execute bounded `table.extract` against
  uniquely scoped semantic table regions.
- [ir-030-067](tasks/ir-030-067.md) — execute guarded `field.read` with bounded
  form-state output and policy-preserving redaction.
- [ir-030-068](tasks/ir-030-068.md) — harden `field.read` with sensitive-value
  redaction coverage and post-observation revision checks.
- [ir-030-069](tasks/ir-030-069.md) — require `inputs.field` during authored
  `field.read` validation.
- [ir-030-070](tasks/ir-030-070.md) — require explicit semantic region scopes
  for browser-backed task families.
- [ir-030-071](tasks/ir-030-071.md) — execute bounded `pagination.collect` with
  revision-aware page advances and recovery guidance.
- [ir-030-072](tasks/ir-030-072.md) — harden extraction revision checks and
  semantic no-op detection for pagination collection.
- [ir-030-073](tasks/ir-030-073.md) — add guarded `navigation.openMenu`
  execution with semantic menu-control targets.
- [ir-030-074](tasks/ir-030-074.md) — verify `navigation.openMenu` outcomes
  through observable expanded state and typed indeterminate recovery.
- [ir-030-075](tasks/ir-030-075.md) — verify `navigation.selectTab` through
  bounded ARIA-selected polling and indeterminate recovery.
- [ir-030-076](tasks/ir-030-076.md) — require a bounded semantic page or route
  transition after `pagination.next`, with delayed-success and no-op recovery
  coverage.
- [ir-030-077](tasks/ir-030-077.md) — verify `navigation.follow` reaches the
  requested destination and return indeterminate recovery for redirects or
  other URL mismatches.
- [ir-030-078](tasks/ir-030-078.md) — restrict `form.submit` to
  evidence-backed semantic button targets and fail closed for named fields or
  other non-submit controls.
- [ir-030-079](tasks/ir-030-079.md) — convert `form.fill` operation and
  post-fill inspection failures into bounded indeterminate recovery results.
- [ir-030-080](tasks/ir-030-080.md) — bound mutation verification failures
  and require explicit `form.submit` postconditions.
- [ir-030-081](tasks/ir-030-081.md) — add typed structured-extraction kinds,
  field-level provenance, and explicit output-limit metadata.
- [ir-030-082](tasks/ir-030-082.md) — add bounded item-level records for
  semantic table and repeated-collection extraction.

- [ir-030-083](tasks/ir-030-083.md) — populate bounded semantic table and
  collection records from accessibility evidence.
- [ir-030-084](tasks/ir-030-084.md) — include structured record changes in
  revision-aware semantic page checks.
- [ir-030-085](tasks/ir-030-085.md) — add bounded, revision-bound
  continuation metadata for truncated extraction.
- [ir-030-086](tasks/ir-030-086.md) — validate continuation revision and route
  before resuming extraction.
- [ir-030-087](tasks/ir-030-087.md) — bind continuations to the requested
  semantic region.
- [ir-030-088](tasks/ir-030-088.md) — bind continuations to the extraction
  field contract.
- [ir-030-089](tasks/ir-030-089.md) — add fail-closed sensitive extraction
  gating for secret-like field names and paths.

## Completed plan: performance overhaul
Status: Complete

The previous plan established compact observation, explicit expensive paths,
browser ownership, stable references, persistent MCP, a responsive TUI, and
baseline performance measurements. Its completed tasks remain below as the
delivery record:

1. [baseline-000](tasks/baseline-000.md)
2. [perf-001](tasks/perf-001.md)
3. [lifecycle-002](tasks/lifecycle-002.md)
4. [action-003](tasks/action-003.md)
5. [mcp-004](tasks/mcp-004.md)
6. [tui-005](tasks/tui-005.md)
7. [verify-006](tasks/verify-006.md)
