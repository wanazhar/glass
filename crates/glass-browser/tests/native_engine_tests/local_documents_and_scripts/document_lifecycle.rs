use super::*;

#[tokio::test]
async fn native_local_document_fragments_preserve_tree_ownership_and_helpers() {
    let mut engine = NativeEngine::new(NativeEngineConfig::default().with_initial_url(
        "data:text/html,%3Cbody%3E%3Cp%20id%3D%27anchor%27%3EA%3C%2Fp%3E%3C%2Fbody%3E",
    ))
    .unwrap();
    engine.initialize_async().await.unwrap();

    let result = engine
        .evaluate_async(
            "(() => { const host = document.createElement('section'); host.id = 'host'; const fragment = document.createDocumentFragment(); const first = document.createElement('b'); first.textContent = 'one'; const second = document.createElement('i'); second.textContent = 'two'; fragment.append(first, '-', second); const staged = [fragment instanceof Node, fragment instanceof DocumentFragment, fragment.ownerDocument === document, fragment.parentNode === null, fragment.childNodes.length, fragment.textContent, first.parentNode === fragment, first.parentElement === null]; host.appendChild(fragment); document.body.insertBefore(host, document.getElementById('anchor')); const marker = document.createElement('em'); marker.textContent = 'marker'; first.before('pre'); first.after(marker); const replacement = document.createElement('u'); replacement.textContent = 'replacement'; second.replaceWith(replacement); host.prepend('start'); const intermediate = host.childNodes.map(child => child.nodeType === 1 ? child.tagName : '#text:' + child.nodeValue); const scratch = document.createDocumentFragment(); const disposable = document.createElement('aside'); scratch.append(disposable); disposable.remove(); const replacementFragment = document.createDocumentFragment(); const finalElement = document.createElement('u'); finalElement.textContent = 'new'; replacementFragment.append(finalElement, 'end'); host.replaceChildren(replacementFragment); return { staged, intermediate, intermediateText: 'startpreonemarker-replacement', intermediateMarkup: 'startpre<b>one</b><em>marker</em>-<u>replacement</u>', final: [fragment.childNodes.length, replacementFragment.childNodes.length, host.innerHTML, host.textContent, host.childNodes.length, host.firstChild.tagName, host.firstChild.parentNode === host, scratch.childNodes.length, disposable.parentNode === null] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "staged": [true, true, true, true, 3, "one-two", true, true],
            "intermediate": ["#text:start", "#text:pre", "B", "EM", "#text:-", "U"],
            "intermediateText": "startpreonemarker-replacement",
            "intermediateMarkup": "startpre<b>one</b><em>marker</em>-<u>replacement</u>",
            "final": [0, 0, "<u>new</u>end", "newend", 2, "U", true, 0, true],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.querySelector('section'); host.id = 'property-id'; host.className = 'alpha beta'; const assigned = [host.id, host.className, host.getAttribute('id'), host.getAttribute('class'), document.getElementById('property-id') === host, document.querySelector('.beta') === host]; host.setAttribute('id', 'attribute-id'); host.removeAttribute('class'); host.title = 'Title'; host.name = 'field'; host.required = true; host.readOnly = true; host.hidden = true; host.disabled = true; host.multiple = true; host.type = 'BUTTON'; const form = [host.title, host.name, host.required, host.readOnly, host.hidden, host.disabled, host.multiple, host.type, host.getAttribute('title'), host.getAttribute('name'), host.getAttribute('required'), host.getAttribute('readonly'), host.getAttribute('hidden'), host.getAttribute('disabled'), host.getAttribute('multiple'), host.getAttribute('type')]; host.required = false; host.readOnly = false; host.hidden = false; host.disabled = false; host.multiple = false; return { assigned, method: [host.id, host.className, host.getAttribute('id'), host.getAttribute('class'), host.hasAttribute('class'), document.getElementById('attribute-id') === host], form, cleared: [host.required, host.readOnly, host.hidden, host.disabled, host.multiple, host.hasAttribute('required'), host.hasAttribute('readonly'), host.hasAttribute('hidden'), host.hasAttribute('disabled'), host.hasAttribute('multiple')] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "assigned": ["property-id", "alpha beta", "property-id", "alpha beta", true, true],
            "method": ["attribute-id", "", "attribute-id", null, false, true],
            "form": ["Title", "field", true, true, true, true, true, "button", "Title", "field", "", "", "", "", "", "BUTTON"],
            "cleared": [false, false, false, false, false, false, false, false, false, false],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const host = document.getElementById('attribute-id'); return [host.id, host.className, host.getAttribute('id'), host.getAttribute('class'), document.querySelector('#attribute-id') === host, host.title, host.name, host.required, host.readOnly, host.hidden, host.disabled, host.multiple, host.type]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["attribute-id", "", "attribute-id", null, true, "Title", "field", false, false, false, false, false, "button"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.createElement('article'); document.body.appendChild(target); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName) })))); observer.observe(target, { childList: true }); target.innerHTML = '<div id=inner><span>Hi &amp; there</span></div>tail'; globalThis.elementInnerHtmlMutationLog = log; return [target.innerHTML, target.textContent, target.querySelector('#inner') !== null, target.querySelector('span').textContent, document.getElementById('inner') !== null, target.childNodes.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["<div id=\"inner\"><span>Hi &amp; there</span></div>tail", "Hi & theretail", true, "Hi & there", true, 2])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.elementInnerHtmlMutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "childList",
                "added": ["div", "#text"],
                "removed": [],
            }
        ]])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.createElement('p'); document.body.appendChild(target); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName) })))); observer.observe(target, { childList: true }); target.textContent = 'native text'; globalThis.elementTextContentMutationLog = log; return [target.textContent, target.firstChild.nodeType, target.firstChild.nodeValue, target.childNodes.length, target.firstChild.parentNode === target]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["native text", 3, "native text", 1, true])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.elementTextContentMutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "childList",
                "added": ["#text"],
                "removed": [],
            }
        ]])
    );
    let parsed = engine
        .evaluate_async(
            "(() => { const fragment = document.createDocumentFragment(); fragment.innerHTML = \"<article data-kind='card'><strong>Hi &amp; there</strong> tail</article><br>\"; const article = fragment.firstElementChild; const strong = fragment.querySelector('strong'); const before = [fragment.innerHTML, fragment.childNodes.length, article.localName, strong.textContent, article.parentNode === fragment, article.parentElement === null]; const mount = document.createElement('main'); mount.appendChild(fragment); document.body.appendChild(mount); return { before, after: [fragment.innerHTML, mount.innerHTML, mount.querySelector('strong') === strong, article.parentNode === mount, article.getAttribute('data-kind')] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        parsed,
        serde_json::json!({
            "before": [
                "<article data-kind=\"card\"><strong>Hi &amp; there</strong> tail</article><br>",
                2,
                "article",
                "Hi & there",
                true,
                true,
            ],
            "after": [
                "",
                "<article data-kind=\"card\"><strong>Hi &amp; there</strong> tail</article><br>",
                true,
                true,
                "card",
            ],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const fragment = document.createDocumentFragment(); fragment.innerHTML = '<!doctype html><!--ignored--><div title=\"a > b\">before<script>if (a < b) x = \"</div>\";</script><textarea>&lt;em&gt;safe&lt;/em&gt;</textarea></div>'; const div = fragment.firstElementChild; const script = div.querySelector('script'); const textarea = div.querySelector('textarea'); return [fragment.childNodes.length, div.getAttribute('title'), script.textContent, textarea.textContent, div.querySelector('em') === null]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([2, "a > b", "if (a < b) x = \"</div>\";", "<em>safe</em>", true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const recovered = document.createDocumentFragment(); recovered.innerHTML = '<!doctype html><!unknown><button ID=\"first\" id=\"second\">Action'; const button = recovered.querySelector('button'); const broken = document.createDocumentFragment(); broken.innerHTML = '<!--unterminated'; const lists = document.createDocumentFragment(); lists.innerHTML = '<ul><li>one<li id=\"second-item\">two</ul><select><option>one<option>two</select><p>first<div>second</div>'; const ul = lists.querySelector('ul'); const select = lists.querySelector('select'); return [recovered.childNodes.length, recovered.firstChild.nodeType, recovered.firstChild.nodeValue, button.id, broken.childNodes.length, broken.firstChild.nodeType, ul.children.length, ul.children[0].textContent, ul.children[1].id, select.children.length, select.children[0].parentNode === select, select.children[1].parentNode === select, lists.querySelector('p').textContent]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([2, 8, "unknown", "first", 1, 8, 2, "one", "second-item", 2, true, true, "first"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const source = document.createElement('section'); source.setAttribute('id', 'moving-source'); const moved = document.createElement('b'); source.appendChild(moved); document.body.appendChild(source); const fragment = document.createDocumentFragment(); fragment.append(moved); const stable = document.createElement('i'); fragment.appendChild(stable); const records = []; const observer = new MutationObserver(entries => records.push(entries.map(record => ({ type: record.type, target: record.target === fragment, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName), previous: record.previousSibling && (record.previousSibling.localName || record.previousSibling.nodeName), next: record.nextSibling && (record.nextSibling.localName || record.nextSibling.nodeName) })))); observer.observe(fragment, { childList: true, subtree: true }); const inserted = document.createElement('em'); fragment.insertBefore(inserted, stable); stable.remove(); globalThis.fragmentMutationLog = records; return [moved.parentNode === fragment, source.childNodes.length, fragment.childNodes.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 0, 2])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.fragmentMutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "childList",
                "target": true,
                "added": ["em"],
                "removed": [],
                "previous": "b",
                "next": "i",
            },
            {
                "type": "childList",
                "target": true,
                "added": [],
                "removed": ["i"],
                "previous": "em",
                "next": null,
            },
        ]])
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('moving-source').childNodes.length")
            .await
            .unwrap(),
        serde_json::json!(0)
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_blob_stylesheet_updates_style_after_late_attachment() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-dynamic-style",
            "<html><head></head><body><p id='target'>Native</p></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-dynamic-style");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.localBlobStyleLog = []; const link = document.createElement('link'); link.rel = 'stylesheet'; const url = URL.createObjectURL(new Blob(['#target { color: rgb(1, 2, 3); }'], { type: 'text/css' })); link.href = url; link.addEventListener('load', () => localBlobStyleLog.push('loaded')); link.addEventListener('error', () => localBlobStyleLog.push('error')); document.head.appendChild(link); return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!("blob:null/glass-native-1")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[localBlobStyleLog, getComputedStyle(document.getElementById('target')).color]",
            )
            .await
            .unwrap(),
        serde_json::json!([["loaded"], "rgb(1, 2, 3)"])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_blob_images_load_and_paint_after_late_attachment() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-dynamic-image",
            "<html><head></head><body><div id='host'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-dynamic-image");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.localBlobImageLog = []; const host = document.getElementById('host'); const image = document.createElement('img'); const surface = document.createElement('div'); surface.id = 'surface'; surface.style.cssText = 'width:2px;height:2px'; const url = URL.createObjectURL(new Blob(['<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"2\"><rect width=\"2\" height=\"2\" fill=\"red\"/></svg>'], { type: 'image/svg+xml' })); image.addEventListener('load', () => localBlobImageLog.push('loaded')); image.addEventListener('error', () => localBlobImageLog.push('error')); image.src = url; surface.style.backgroundImage = `url(\"${url}\")`; host.appendChild(image); host.appendChild(surface); globalThis.localBlobImage = image; return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!("blob:null/glass-native-1")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = globalThis.localBlobImage; return [localBlobImageLog, image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([[
            "loaded"
        ], true, 2, 2, "blob:null/glass-native-1"])
    );
    assert_eq!(
        engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
            .count(),
        2
    );

    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_blob_media_exposes_metadata_and_load_event() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-dynamic-media",
            "<html><head></head><body><div id='host'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-dynamic-media");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.localBlobMediaLog = [];
                    const bytes = new Uint8Array(44 + 8000);
                    const view = new DataView(bytes.buffer);
                    const write = (offset, value) => [...value].forEach((character, index) => view.setUint8(offset + index, character.charCodeAt(0)));
                    write(0, 'RIFF');
                    view.setUint32(4, 8036, true);
                    write(8, 'WAVE');
                    write(12, 'fmt ');
                    view.setUint32(16, 16, true);
                    view.setUint16(20, 1, true);
                    view.setUint16(22, 1, true);
                    view.setUint32(24, 8000, true);
                    view.setUint32(28, 8000, true);
                    view.setUint16(32, 1, true);
                    view.setUint16(34, 8, true);
                    write(36, 'data');
                    view.setUint32(40, 8000, true);
                    bytes.fill(128, 44);
                    const audio = document.createElement('audio');
                    audio.addEventListener('load', () => localBlobMediaLog.push('load'));
                    audio.addEventListener('error', () => localBlobMediaLog.push('error'));
                    audio.src = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }));
                    document.getElementById('host').appendChild(audio);
                    globalThis.localBlobAudio = audio;
                    return audio.src;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!("blob:null/glass-native-1")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = localBlobAudio; return [localBlobMediaLog, audio instanceof HTMLAudioElement, audio.readyState, audio.networkState, audio.duration, audio.currentSrc, audio.error, audio.buffered.length, audio.buffered.end(0), audio.canPlayType('audio/wav')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["load"],
            true,
            1,
            1,
            1,
            "blob:null/glass-native-1",
            serde_json::Value::Null,
            1,
            1,
            "maybe"
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_data_media_loads_during_navigation() {
    let mut wav = vec![0_u8; 44 + 320];
    wav[0..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&(356_u32).to_le_bytes());
    wav[8..12].copy_from_slice(b"WAVE");
    wav[12..16].copy_from_slice(b"fmt ");
    wav[16..20].copy_from_slice(&(16_u32).to_le_bytes());
    wav[20..22].copy_from_slice(&(1_u16).to_le_bytes());
    wav[22..24].copy_from_slice(&(1_u16).to_le_bytes());
    wav[24..28].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[28..32].copy_from_slice(&(8000_u32).to_le_bytes());
    wav[32..34].copy_from_slice(&(1_u16).to_le_bytes());
    wav[34..36].copy_from_slice(&(8_u16).to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&(320_u32).to_le_bytes());
    wav[44..].fill(128);
    let data_url = format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&wav)
    );
    let html = format!("<audio id='audio' src='{data_url}'></audio>");
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://data-media", html)
        .unwrap()
        .with_initial_url("fixture://data-media");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); return [audio.readyState, audio.networkState, audio.currentSrc, audio.duration, audio.error, audio.canPlayType('audio/wav')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([1, 1, data_url, 0.04, serde_json::Value::Null, "maybe"])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_blob_media_play_resolves_for_known_duration() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-media-playback",
            "<html><head></head><body><div id='host'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-media-playback");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.mediaPlaybackLog = [];
                    const bytes = new Uint8Array(44 + 8000);
                    const view = new DataView(bytes.buffer);
                    const write = (offset, value) => [...value].forEach((character, index) => view.setUint8(offset + index, character.charCodeAt(0)));
                    write(0, 'RIFF');
                    view.setUint32(4, 8036, true);
                    write(8, 'WAVE');
                    write(12, 'fmt ');
                    view.setUint32(16, 16, true);
                    view.setUint16(20, 1, true);
                    view.setUint16(22, 1, true);
                    view.setUint32(24, 8000, true);
                    view.setUint32(28, 8000, true);
                    view.setUint16(32, 1, true);
                    view.setUint16(34, 8, true);
                    write(36, 'data');
                    view.setUint32(40, 8000, true);
                    bytes.fill(128, 44);
                    const audio = document.createElement('audio');
                    for (const type of ['play', 'playing', 'pause', 'ended']) audio.addEventListener(type, () => mediaPlaybackLog.push(type));
                    audio.src = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }));
                    document.getElementById('host').appendChild(audio);
                    globalThis.mediaPlaybackAudio = audio;
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await mediaPlaybackAudio.play().then(() => [mediaPlaybackLog, mediaPlaybackAudio.paused, mediaPlaybackAudio.ended])",
            )
            .await
            .unwrap(),
        serde_json::json!([["play", "playing"], false, false])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { mediaPlaybackAudio.pause(); return [mediaPlaybackLog, mediaPlaybackAudio.paused]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([["play", "playing", "pause"], true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { mediaPlaybackAudio.currentTime = 0.25; return [mediaPlaybackAudio.currentTime, mediaPlaybackAudio.played.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([0.25, 1])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await mediaPlaybackAudio.play().then(() => [mediaPlaybackAudio.paused, mediaPlaybackAudio.ended])",
            )
            .await
            .unwrap(),
        serde_json::json!([false, false])
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const time = mediaPlaybackAudio.currentTime; return [time > 0.25, time < 1, mediaPlaybackAudio.ended, mediaPlaybackAudio.played.length, mediaPlaybackAudio.played.end(0) > 0.25]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, false, 1, true])
    );
    engine
            .evaluate_async(
            "(() => { mediaPlaybackAudio.playbackRate = 100; mediaPlaybackAudio.play(); return true; })()",
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        engine
            .evaluate_async(
                "[mediaPlaybackAudio.currentTime, mediaPlaybackAudio.ended, mediaPlaybackAudio.paused, mediaPlaybackLog.includes('ended')]",
            )
            .await
            .unwrap(),
        serde_json::json!([1, true, true, true])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_blob_media_selects_supported_source() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-media-source-selection",
            "<html><head></head><body><div id='host'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-media-source-selection");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.mediaSourceLog = [];
                    const audio = document.createElement('audio');
                    const unsupported = document.createElement('source');
                    unsupported.type = 'audio/x-not-supported';
                    unsupported.src = URL.createObjectURL(new Blob(['bad'], { type: 'audio/x-not-supported' }));
                    const supported = document.createElement('source');
                    supported.type = 'audio/webm';
                    supported.src = URL.createObjectURL(new Blob([new Uint8Array([0x1a, 0x45, 0xdf, 0xa3])], { type: 'audio/webm' }));
                    audio.addEventListener('load', () => mediaSourceLog.push('load'));
                    audio.addEventListener('error', () => mediaSourceLog.push('error'));
                    audio.append(unsupported, supported);
                    document.getElementById('host').appendChild(audio);
                    globalThis.mediaSourceAudio = audio;
                    return [unsupported.src, supported.src];
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!([
            "blob:null/glass-native-1",
            "blob:null/glass-native-2"
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[mediaSourceLog, mediaSourceAudio.readyState, mediaSourceAudio.networkState, mediaSourceAudio.currentSrc, mediaSourceAudio.error]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["load"],
            1,
            1,
            "blob:null/glass-native-2",
            serde_json::Value::Null
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await mediaSourceAudio.play().then(() => 'played', error => error.name)",
            )
            .await
            .unwrap(),
        serde_json::json!("NotSupportedError")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dynamic_blob_media_reports_unsupported_payload_error() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-media-error",
            "<html><head></head><body><div id='host'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-media-error");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.mediaErrorLog = [];
                    const audio = document.createElement('audio');
                    audio.addEventListener('error', () => mediaErrorLog.push('error'));
                    audio.src = URL.createObjectURL(new Blob(['not a media resource']));
                    document.getElementById('host').appendChild(audio);
                    globalThis.mediaErrorAudio = audio;
                    return audio.src;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!("blob:null/glass-native-1")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[mediaErrorLog, mediaErrorAudio.readyState, mediaErrorAudio.networkState, mediaErrorAudio.currentSrc, mediaErrorAudio.error && mediaErrorAudio.error.code]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            ["error"],
            0,
            3,
            "blob:null/glass-native-1",
            4
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_message_ports_support_transfer_lists_and_structured_clone() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-message-port-transfer",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://local-message-port-transfer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.received = null;
                    globalThis.reply = null;
                    globalThis.cloneReceived = null;
                    globalThis.bufferReceived = null;
                    const receiving = new MessageChannel();
                    receiving.port2.onmessage = event => {
                        received = [event.data.kind, event.ports.length,
                            event.data.port === event.ports[0],
                            event.ports[0] instanceof MessagePort];
                        event.ports[0].postMessage({ kind: 'reply' });
                    };
                    receiving.port2.start();
                    const offered = new MessageChannel();
                    offered.port2.onmessage = event => { reply = event.data.kind; };
                    offered.port2.start();
                    receiving.port1.postMessage({ kind: 'port', port: offered.port1 }, [offered.port1]);
                    let sourceError = '';
                    try { offered.port1.postMessage({ kind: 'invalid' }); }
                    catch (error) { sourceError = error.name; }
                    const bufferChannel = new MessageChannel();
                    bufferChannel.port2.onmessage = event => {
                        const buffer = event.data.buffer;
                        bufferReceived = [buffer instanceof ArrayBuffer,
                            Array.from(new Uint8Array(buffer))];
                    };
                    bufferChannel.port2.start();
                    const buffer = new ArrayBuffer(3);
                    new Uint8Array(buffer).set([6, 7, 8]);
                    bufferChannel.port1.postMessage({ buffer }, [buffer]);
                    globalThis.bufferSource = buffer.byteLength;
                    const cloned = new MessageChannel();
                    cloned.port2.onmessage = event => { cloneReceived = event.data.kind; };
                    cloned.port2.start();
                    const clonedPort = structuredClone(cloned.port1, { transfer: [cloned.port1] });
                    clonedPort.postMessage({ kind: 'clone' });
                    let cloneSourceError = '';
                    try { cloned.port1.postMessage({ kind: 'invalid' }); }
                    catch (error) { cloneSourceError = error.name; }
                    globalThis.transferState = [sourceError, cloneSourceError];
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "({ received, reply, bufferReceived, bufferSource, cloneReceived, transferState })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "received": ["port", 1, true, true],
            "reply": "reply",
            "bufferReceived": [true, [6, 7, 8]],
            "bufferSource": 0,
            "cloneReceived": "clone",
            "transferState": ["InvalidStateError", "InvalidStateError"],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_blob_object_urls_feed_fetch_and_xhr_and_revoke() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://blob-object-url-page", "<p>Blob object URL</p>")
        .unwrap()
        .with_initial_url("fixture://blob-object-url-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const blob = new Blob(['native object URL'], { type: 'text/plain' });
                    const url = URL.createObjectURL(blob);
                    const response = await fetch(url);
                    const head = await fetch(url, { method: 'HEAD' });
                    const xhr = await new Promise((resolve, reject) => {
                        const request = new XMLHttpRequest();
                        request.onload = () => resolve([request.status, request.responseURL, request.responseText]);
                        request.onerror = () => reject(new Error('object URL XHR failed'));
                        request.open('GET', url);
                        request.send();
                    });
                    const sync = new XMLHttpRequest();
                    sync.open('GET', url, false);
                    sync.send();
                    const fetched = await response.text();
                    URL.revokeObjectURL(url);
                    const revoked = await fetch(url).then(() => false, () => true);
                    let invalid = '';
                    try { URL.createObjectURL('not-a-blob'); } catch (error) { invalid = error.name; }
                    return {
                        url: url.startsWith('blob:'),
                        response: [response.status, response.url, response.headers.get('content-type'), response.headers.get('content-length'), fetched],
                        head: [head.status, head.body === null, head.headers.get('content-length')],
                        xhr,
                        sync: [sync.status, sync.responseURL, sync.responseText],
                        revoked,
                        invalid,
                    };
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "url": true,
            "response": [200, "blob:null/glass-native-1", "text/plain", "17", "native object URL"],
            "head": [200, true, "17"],
            "xhr": [200, "blob:null/glass-native-1", "native object URL"],
            "sync": [200, "blob:null/glass-native-1", "native object URL"],
            "revoked": true,
            "invalid": "TypeError",
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_blob_object_urls_navigate_to_a_fresh_document() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-document-start",
            "<title>Start</title><p>Before Blob navigation</p>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-document-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let object_url = engine
        .evaluate_async(
            r#"(() => {
                const url = URL.createObjectURL(new Blob([
                    '<title>Blob document</title><main>Loaded from an object URL</main>'
                ], { type: 'text/html' }));
                location.assign(url);
                return url;
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(object_url, serde_json::json!("blob:null/glass-native-1"));
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, "blob:null/glass-native-1");
    assert_eq!(snapshot.origin, NativeOrigin::Opaque);
    assert_eq!(snapshot.title, "Blob document");
    assert_eq!(snapshot.visible_text, "Loaded from an object URL");
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_exposes_os_seeded_crypto() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://page-crypto",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://page-crypto");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const values = new Uint8Array(8);
                    crypto.getRandomValues(values);
                    const uuid = crypto.randomUUID();
                    const bulk = new Uint8Array(16 * 1024);
                    crypto.getRandomValues(bulk);
                    const refill = new Uint8Array(16);
                    crypto.getRandomValues(refill);
                    let floatRejected = false;
                    try { crypto.getRandomValues(new Float32Array(1)); } catch (error) {
                        floatRejected = error instanceof TypeError;
                    }
                    let largeRejected = false;
                    try { crypto.getRandomValues(new Uint8Array(65536 + 1)); } catch (error) {
                        largeRejected = error.name === 'QuotaExceededError';
                    }
                    globalThis.pageCryptoState = {
                        byteLength: values.byteLength,
                        bulkByteLength: bulk.byteLength,
                        refillByteLength: refill.byteLength,
                        changed: values.some(value => value !== 0),
                        uuidValid: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(uuid),
                        uuidVersion: uuid[14],
                        uuidVariant: ['8', '9', 'a', 'b'].includes(uuid[19]),
                        floatRejected,
                        largeRejected,
                        cryptoIdentity: crypto === globalThis.crypto,
                    };
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.pageCryptoState")
            .await
            .unwrap(),
        serde_json::json!({
            "byteLength": 8,
            "bulkByteLength": 16384,
            "refillByteLength": 16,
            "changed": true,
            "uuidValid": true,
            "uuidVersion": "4",
            "uuidVariant": true,
            "floatRejected": true,
            "largeRejected": true,
            "cryptoIdentity": true,
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_exposes_standard_runtime_primitives() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://page-primitives",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://page-primitives");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const bytes = new TextEncoder().encode('hé');
                    const destination = new Uint8Array(4);
                    const encoded = new TextEncoder().encodeInto('hi', destination);
                    const decoded = new TextDecoder().decode(bytes);
                    const target = new EventTarget();
                    const seen = [];
                    const listener = { handleEvent(event) {
                        seen.push([event.type, event.target === target, event.currentTarget === target]);
                        event.preventDefault();
                    }};
                    target.addEventListener('ping', listener);
                    const dispatched = target.dispatchEvent(new Event('ping', { cancelable: true }));
                    target.removeEventListener('ping', listener);
                    target.dispatchEvent(new Event('ping', { cancelable: true }));
                    const clone = structuredClone({ nested: ['ok'] });
                    const base64 = btoa('hello');
                    let decoderRejected = false;
                    try { new TextDecoder('iso-8859-1'); } catch (error) {
                        decoderRejected = error instanceof RangeError;
                    }
                    globalThis.pagePrimitiveState = {
                        bytes: Array.from(bytes),
                        decoded,
                        encoded: [encoded.read, encoded.written, Array.from(destination)],
                        events: [seen, dispatched],
                        clone: clone.nested[0],
                        base64: [base64, atob(base64)],
                        constructors: [typeof TextEncoder, typeof TextDecoder, typeof EventTarget, typeof Event, typeof structuredClone, typeof queueMicrotask],
                        decoderRejected,
                    };
                    queueMicrotask(() => { globalThis.pagePrimitiveState.microtask = true; });
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.pagePrimitiveState")
            .await
            .unwrap(),
        serde_json::json!({
            "bytes": [104, 195, 169],
            "decoded": "hé",
            "encoded": [2, 2, [104, 105, 0, 0]],
            "events": [[["ping", true, true]], false],
            "clone": "ok",
            "base64": ["aGVsbG8=", "hello"],
            "constructors": ["function", "function", "function", "function", "function", "function"],
            "decoderRejected": true,
            "microtask": true,
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_sync_xhr_uses_fixture_loader() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://sync-xhr.test/page", "<main>Sync</main>")
        .unwrap()
        .with_fixture("fixture://sync-xhr.test/response", "fixture-sync-response")
        .unwrap()
        .with_initial_url("fixture://sync-xhr.test/page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const xhr = new XMLHttpRequest(); const states = []; xhr.onreadystatechange = () => states.push(xhr.readyState); xhr.open('REPORT', '/response', false); xhr.send(); return { status: xhr.status, statusText: xhr.statusText, responseText: xhr.responseText, response: xhr.response, responseURL: xhr.responseURL, states, readyState: xhr.readyState }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "status": 200,
            "statusText": "OK",
            "responseText": "fixture-sync-response",
            "response": "fixture-sync-response",
            "responseURL": "fixture://sync-xhr.test/response",
            "states": [1, 4],
            "readyState": 4,
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_full_navigation_orders_page_lifecycle_events() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://lifecycle-old",
            "<script>addEventListener('pagehide', () => globalThis.oldPagehide = true); addEventListener('unload', () => globalThis.oldUnload = true);</script><p>Old</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://lifecycle-next",
            "<script>globalThis.lifecycle = []; addEventListener('pageshow', event => lifecycle.push([document.readyState, event.persisted]));</script><p>Next</p>",
        )
        .unwrap()
        .with_initial_url("fixture://lifecycle-old");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let before_navigation = engine.revision();
    engine
        .navigate_async("fixture://lifecycle-next")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.lifecycle").await.unwrap(),
        serde_json::json!([["complete", false]])
    );
    let lifecycle_effects = engine
        .effects_since(before_navigation)
        .unwrap()
        .effects
        .into_iter()
        .map(|effect| effect.kind)
        .filter(|kind| {
            matches!(
                kind,
                NativeEventKind::PageHide | NativeEventKind::Unload | NativeEventKind::PageShow
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle_effects,
        vec![
            NativeEventKind::PageHide,
            NativeEventKind::Unload,
            NativeEventKind::PageShow
        ]
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_beforeunload_without_activation_does_not_block_navigation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://beforeunload-start",
            "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'page-authored warning'; });</script><button id='activate' onclick='globalThis.clicked = true'>Activate</button><title>Start</title>",
        )
        .unwrap()
        .with_fixture("fixture://beforeunload-next", "<title>Next</title>")
        .unwrap()
        .with_initial_url("fixture://beforeunload-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('activate').click()")
        .await
        .unwrap();
    engine
        .navigate_async("fixture://beforeunload-next")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://beforeunload-next"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Next");
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_outgoing_lifecycle_can_replace_navigation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://lifecycle-reentry-start",
            "<script>globalThis.lifecycle = []; addEventListener('beforeunload', () => { lifecycle.push('before'); location.replace('fixture://lifecycle-reentry-final'); }); addEventListener('pagehide', () => lifecycle.push('hide')); addEventListener('unload', () => lifecycle.push('unload'));</script><title>Start</title>",
        )
        .unwrap()
        .with_fixture(
            "fixture://lifecycle-reentry-final",
            "<title>Final</title><p>Published</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://lifecycle-reentry-requested",
            "<title>Requested</title><p>Should not win</p>",
        )
        .unwrap()
        .with_initial_url("fixture://lifecycle-reentry-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let before_navigation = engine.revision();

    engine
        .navigate_async("fixture://lifecycle-reentry-requested")
        .await
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://lifecycle-reentry-final"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Final");
    assert_eq!(engine.history().len(), 1);
    let lifecycle_effects = engine
        .effects_since(before_navigation)
        .unwrap()
        .effects
        .into_iter()
        .map(|effect| effect.kind)
        .filter(|kind| {
            matches!(
                kind,
                NativeEventKind::BeforeUnload | NativeEventKind::PageHide | NativeEventKind::Unload
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle_effects,
        vec![
            NativeEventKind::BeforeUnload,
            NativeEventKind::PageHide,
            NativeEventKind::Unload
        ]
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_hashchange_can_reenter_navigation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://hash-reentry",
            "<script>addEventListener('hashchange', () => { if (location.hash === '#two') location.assign('fixture://hash-reentry-final'); });</script><title>Start</title><p>Start</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://hash-reentry-final",
            "<title>Final</title><p>Published</p>",
        )
        .unwrap()
        .with_initial_url("fixture://hash-reentry#one");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .navigate_async("fixture://hash-reentry#two")
        .await
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://hash-reentry-final"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Final");
    assert_eq!(engine.history().len(), 3);
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_history_traversal_orders_popstate_before_hashchange() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://popstate-page",
            "<script>globalThis.events = []; addEventListener('popstate', event => events.push(event.type)); addEventListener('hashchange', event => events.push(event.type));</script><p id='one'>One</p><p id='two'>Two</p>",
        )
        .unwrap()
        .with_initial_url("fixture://popstate-page#one");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async("fixture://popstate-page#two")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["hashchange"])
    );
    engine.go_back().unwrap().unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["hashchange", "popstate", "hashchange"])
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://popstate-page#one"
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_fragment_navigation_dispatches_hashchange_without_reload() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://hash-page",
            "<script>globalThis.hashEvents = []; addEventListener('hashchange', (event) => hashEvents.push({ oldURL: event.oldURL, newURL: event.newURL, href: location.href }));</script><p id='target'>Hash target</p>",
        )
        .unwrap()
        .with_initial_url("fixture://hash-page#one");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let before_navigation = engine.revision();
    engine
        .navigate_async("fixture://hash-page#two")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.hashEvents")
            .await
            .unwrap(),
        serde_json::json!([{
            "oldURL": "fixture://hash-page#one",
            "newURL": "fixture://hash-page#two",
            "href": "fixture://hash-page#two"
        }])
    );
    assert_eq!(engine.snapshot().unwrap().url, "fixture://hash-page#two");
    let effects = engine.effects_since(before_navigation).unwrap().effects;
    assert!(
        effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::HashChange)
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_hashchange_accepts_large_urls_without_source_coupling() {
    let initial_fragment = "a".repeat(8200);
    let next_fragment = format!("{}b", "a".repeat(8199));
    let initial_url = format!("fixture://hash-large#{initial_fragment}");
    let next_url = format!("fixture://hash-large#{next_fragment}");
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://hash-large",
            "<script>globalThis.hashEvents = []; addEventListener('hashchange', event => hashEvents.push([event.oldURL.length, event.newURL.length, event.oldURL.endsWith('a'), event.newURL.endsWith('b')]));</script><p>Large hash</p>",
        )
        .unwrap()
        .with_initial_url(initial_url.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine.navigate_async(next_url.clone()).await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("globalThis.hashEvents")
            .await
            .unwrap(),
        serde_json::json!([[initial_url.len(), next_url.len(), true, true]])
    );
    assert_eq!(engine.snapshot().unwrap().url, next_url);
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_idle_callbacks_honor_cancellation_and_deadline() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://idle-callbacks",
            r#"
                <script>
                    globalThis.idleLog = [];
                    const cancelled = requestIdleCallback(() => idleLog.push('cancelled'));
                    cancelIdleCallback(cancelled);
                    requestIdleCallback(function(deadline) {
                        idleLog.push([
                            'timeout',
                            deadline.didTimeout,
                            typeof deadline.timeRemaining() === 'number',
                            deadline.timeRemaining() >= 0,
                            this === window,
                        ]);
                    }, { timeout: 0 });
                    requestIdleCallback(function(deadline) {
                        idleLog.push([
                            'idle',
                            deadline.didTimeout,
                            typeof deadline.timeRemaining() === 'number',
                            deadline.timeRemaining() >= 0,
                            this === window,
                        ]);
                    });
                </script>
                <title>Idle callbacks are observable</title>
                <p>Document committed</p>
            "#,
        )
        .unwrap()
        .with_initial_url("fixture://idle-callbacks");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine.evaluate_async("globalThis.idleLog").await.unwrap(),
        serde_json::json!([
            ["timeout", true, true, true, true],
            ["idle", false, true, true, true],
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_scheduled_callback_errors_do_not_abort_sibling_work() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://scheduled-callback-errors",
            r#"
                <script>
                    globalThis.eventErrors = [];
                    globalThis.handlerErrors = [];
                    globalThis.callbackErrors = {};
                    globalThis.timerSurvived = false;
                    window.addEventListener('error', event => eventErrors.push([
                        event.message,
                        event.error instanceof Error,
                        event.target === window,
                        event.error === callbackErrors[event.message],
                    ]));
                    window.onerror = (message, filename, line, column, error) => handlerErrors.push([
                        message,
                        error instanceof Error,
                        error === callbackErrors[message],
                    ]);
                    setTimeout(() => { const error = new Error('timer callback boom'); callbackErrors[error.message] = error; throw error; }, 0);
                    setTimeout(() => { globalThis.timerSurvived = true; }, 0);
                    requestIdleCallback(() => { const error = new Error('idle callback boom'); callbackErrors[error.message] = error; throw error; });
                    requestAnimationFrame(() => { const error = new Error('frame callback boom'); callbackErrors[error.message] = error; throw error; });
                </script>
                <p>Scheduled callbacks remain isolated</p>
            "#,
        )
        .unwrap()
        .with_initial_url("fixture://scheduled-callback-errors");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        engine
            .evaluate_async("({ eventErrors: eventErrors.slice().sort((left, right) => left[0].localeCompare(right[0])), handlerErrors: handlerErrors.slice().sort((left, right) => left[0].localeCompare(right[0])), timerSurvived })")
            .await
            .unwrap(),
        serde_json::json!({
            "eventErrors": [
                ["frame callback boom", true, true, true],
                ["idle callback boom", true, true, true],
                ["timer callback boom", true, true, true],
            ],
            "handlerErrors": [
                ["frame callback boom", true, true],
                ["idle callback boom", true, true],
                ["timer callback boom", true, true],
            ],
            "timerSurvived": true,
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_history_restores_nested_scroll_offsets() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 120,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://nested-history",
            "<html><body><div id='scroller' style='width:40px;height:30px;overflow:auto'><div style='width:80px;height:90px'></div></div><div style='height:140px'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://nested-history");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo({ left: 20, top: 25 }); window.scrollTo({ top: 40 }); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 40])
    );
    assert_eq!(engine.history().entries()[0].nested_scroll_offsets.len(), 1);

    engine
        .evaluate_async(
            "history.pushState({ view: 'next' }, '', '?view=next'); ({ length: history.length, url: location.href })",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo(0, 0); window.scrollTo(0, 0); return true; })()",
        )
        .await
        .unwrap();

    engine.evaluate_async("history.back(); true").await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.scrollLeft, scroller.scrollTop, window.scrollX, window.scrollY, document.documentElement.scrollTop]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 0, 40, 40])
    );
    assert_eq!(
        engine
            .history()
            .current()
            .unwrap()
            .nested_scroll_offsets
            .len(),
        1
    );

    engine
        .evaluate_async("history.forward(); true")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([0, 0, 0])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_history_restores_nested_scroll_offsets_across_resource_navigation() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 120,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://nested-history-a",
            "<html><body><div id='scroller' style='width:40px;height:30px;overflow:auto'><div style='width:80px;height:90px'></div></div><div style='height:140px'></div></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://nested-history-b",
            "<html><body><p>Second document</p></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://nested-history-a");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo({ left: 20, top: 25 }); window.scrollTo({ top: 40 }); return true; })()",
        )
        .await
        .unwrap();

    engine
        .navigate_async("fixture://nested-history-b")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, "fixture://nested-history-b");
    engine.go_back_async().await.unwrap().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 40])
    );
    assert_eq!(
        engine
            .history()
            .current()
            .unwrap()
            .nested_scroll_offsets
            .len(),
        1
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_action_event_preserves_history_api() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://history-action",
            "<button id='push'>Push</button><script>document.getElementById('push').addEventListener('click', () => history.pushState({ view: 'next' }, '', '/next'));</script>",
        )
        .unwrap()
        .with_initial_url("fixture://history-action");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .action(NativeAction::Click {
            target: "id=push".into(),
        })
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://history-action/next"
    );
    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "length": 2,
            "state": {"view": "next"},
            "url": "fixture://history-action/next",
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_delayed_timer_waits_for_due_host_turn() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://delayed-timer",
            "<script>globalThis.taskOrder = 'script'; setTimeout(() => { globalThis.taskOrder += '-timer'; }, 200);</script>",
        )
        .unwrap()
        .with_initial_url("fixture://delayed-timer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.taskOrder").await.unwrap(),
        serde_json::json!("script")
    );
    tokio::time::sleep(Duration::from_millis(220)).await;
    assert_eq!(
        engine.evaluate_async("globalThis.taskOrder").await.unwrap(),
        serde_json::json!("script-timer")
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_interval_reschedules_until_cleared() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://interval-timer",
            "<script>globalThis.intervalCount = 0; globalThis.intervalId = setInterval(() => { globalThis.intervalCount += 1; }, 40);</script>",
        )
        .unwrap()
        .with_initial_url("fixture://interval-timer");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.intervalCount")
            .await
            .unwrap(),
        serde_json::json!(0)
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    let first_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap();
    assert!(
        first_count.as_i64().is_some_and(|count| count >= 1),
        "interval did not fire: {first_count:?}"
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    let second_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap();
    assert!(
        second_count
            .as_i64()
            .zip(first_count.as_i64())
            .is_some_and(|(second, first)| second > first),
        "interval did not reschedule: first={first_count:?}, second={second_count:?}"
    );
    engine
        .evaluate_async("clearInterval(globalThis.intervalId)")
        .await
        .unwrap();
    let cleared_count = engine
        .evaluate_async("globalThis.intervalCount")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        engine
            .evaluate_async("globalThis.intervalCount")
            .await
            .unwrap(),
        cleared_count
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_abort_signal_timeout_and_any_follow_host_turns() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://abort-signal-static", "<p>Abort signals</p>")
        .unwrap()
        .with_initial_url("fixture://abort-signal-static");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                const timeoutSignal = AbortSignal.timeout(2000);
                const controller = new AbortController();
                const combined = AbortSignal.any([timeoutSignal, controller.signal]);
                const events = [];
                combined.addEventListener('abort', () => events.push(combined.reason.name));
                const immediateController = new AbortController();
                const immediate = AbortSignal.any([immediateController.signal]);
                immediateController.abort();
                const staticAbort = AbortSignal.abort('static-reason');
                globalThis.abortSignalState = { timeoutSignal, combined, events, immediate, empty: AbortSignal.any([]), staticAbort };
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[abortSignalState.timeoutSignal.aborted, abortSignalState.combined.aborted, abortSignalState.immediate.aborted, abortSignalState.empty.aborted, abortSignalState.staticAbort.aborted, abortSignalState.staticAbort.reason]",
            )
            .await
            .unwrap(),
        serde_json::json!([false, false, true, false, true, "static-reason"])
    );
    tokio::time::sleep(Duration::from_millis(2030)).await;
    assert_eq!(
        engine
            .evaluate_async(
                "[abortSignalState.timeoutSignal.aborted, abortSignalState.timeoutSignal.reason.name, abortSignalState.combined.aborted, abortSignalState.combined.reason.name, abortSignalState.events]",
            )
            .await
            .unwrap(),
        serde_json::json!([true, "TimeoutError", true, "TimeoutError", ["TimeoutError"]])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_url_search_params_iterators_are_live_and_self_iterating() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://url-search-params-live", "<p>URLSearchParams</p>")
        .unwrap()
        .with_initial_url("fixture://url-search-params-live");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                const params = new URLSearchParams([['a', 'one'], ['b', 'two']]);
                const entries = params.entries();
                const first = entries.next();
                params.set('b', 'updated');
                params.append('c', 'three');
                const second = entries.next();
                const third = entries.next();
                const complete = entries.next();
                const keys = params.keys();
                const firstKey = keys.next();
                params.append('d', 'four');
                const remainingKeys = [keys.next().value, keys.next().value, keys.next().value, keys.next().done];
                globalThis.liveUrlSearchParams = [
                    params[Symbol.iterator] === params.entries,
                    entries[Symbol.iterator]() === entries,
                    first.value,
                    first.done,
                    second.value,
                    third.value,
                    complete.done,
                    firstKey.value,
                    remainingKeys,
                    params.has('b', 'updated'),
                    params.has('b', 'two'),
                ];
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.liveUrlSearchParams")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            true,
            ["a", "one"],
            false,
            ["b", "updated"],
            ["c", "three"],
            true,
            "a",
            ["b", "c", "d", true],
            true,
            false,
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_request_headers_iterators_are_live_and_self_iterating() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://headers-live", "<p>Headers</p>")
        .unwrap()
        .with_initial_url("fixture://headers-live");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                const headers = new Headers([['X-First', 'one'], ['X-Second', 'two']]);
                const entries = headers.entries();
                const first = entries.next();
                headers.set('x-second', 'updated');
                headers.append('X-Third', 'three');
                const second = entries.next();
                const third = entries.next();
                const complete = entries.next();
                const keys = headers.keys();
                const firstKey = keys.next();
                headers.append('X-Fourth', 'four');
                const remainingKeys = [keys.next().value, keys.next().value, keys.next().value, keys.next().done];
                globalThis.liveHeaders = [
                    headers[Symbol.iterator] === headers.entries,
                    entries[Symbol.iterator]() === entries,
                    first.value,
                    first.done,
                    second.value,
                    third.value,
                    complete.done,
                    firstKey.value,
                    remainingKeys,
                ];
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.liveHeaders")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            true,
            ["x-first", "one"],
            false,
            ["x-second", "updated"],
            ["x-third", "three"],
            true,
            "x-first",
            ["x-second", "x-third", "x-fourth", true],
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_response_objects_expose_bounded_constructors_and_identity() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://response-objects", "<p>Responses</p>")
        .unwrap()
        .with_initial_url("fixture://response-objects");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(async () => {
                const constructed = new Response(new Uint8Array([0, 255, 128, 65]), {
                    status: 201,
                    statusText: 'Created',
                    headers: { 'Content-Type': 'application/octet-stream' },
                });
                const json = Response.json({ ready: true });
                const empty = new Response();
                const error = Response.error();
                const redirect = Response.redirect('https://example.test/next', 307);
                const clone = constructed.clone();
                let headerMutation;
                try { constructed.headers.set('x-test', 'blocked'); } catch (error) { headerMutation = [error.name, error.message]; }
                globalThis.responseObjects = [
                    constructed instanceof Response,
                    constructed.headers instanceof Headers,
                    constructed.status,
                    constructed.statusText,
                    constructed.headers.get('content-type'),
                    headerMutation,
                    Array.from(await constructed.bytes()),
                    await json.text(),
                    json.headers.get('content-type'),
                    empty.body === null,
                    error.type,
                    error.status,
                    error.body === null,
                    redirect.status,
                    redirect.headers.get('location'),
                    redirect.body === null,
                    clone !== constructed,
                    Array.from(await clone.bytes()),
                ];
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.responseObjects")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            true,
            201,
            "Created",
            "application/octet-stream",
            ["TypeError", "native response Headers are immutable"],
            [0, 255, 128, 65],
            "{\"ready\":true}",
            "application/json",
            true,
            "error",
            0,
            true,
            307,
            "https://example.test/next",
            true,
            true,
            [0, 255, 128, 65],
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_response_objects_support_stream_bodies_and_tee() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://response-streams", "<p>Response streams</p>")
        .unwrap()
        .with_initial_url("fixture://response-streams");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(async () => {
                const makeStream = () => new ReadableStream({
                    start(controller) {
                        controller.enqueue('response-');
                        controller.enqueue('body');
                        controller.close();
                    },
                });
                const input = makeStream();
                const response = new Response(input, { headers: { 'Content-Type': 'text/plain' } });
                const sameBody = response.body === input;
                const clone = response.clone();
                const initial = [response.bodyUsed, clone.bodyUsed];
                const text = await response.text();
                const cloneText = await clone.text();
                const after = [response.bodyUsed, clone.bodyUsed];
                const responseAgain = await response.text().then(() => 'readable', error => error.name);
                const cloneAgain = await clone.text().then(() => 'readable', error => error.name);
                const lockedInput = makeStream();
                const lockedReader = lockedInput.getReader();
                const lockedResponse = (() => {
                    try { new Response(lockedInput); return 'constructable'; }
                    catch (error) { return error.name; }
                })();
                lockedReader.releaseLock();
                globalThis.responseStreamObjects = [
                    sameBody,
                    initial,
                    text,
                    cloneText,
                    after,
                    responseAgain,
                    cloneAgain,
                    lockedResponse,
                ];
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.responseStreamObjects")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            [false, false],
            "response-body",
            "response-body",
            [true, true],
            "TypeError",
            "TypeError",
            "TypeError",
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_streams_pipe_to_bounded_writable_sinks() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://stream-piping", "<p>Stream piping</p>")
        .unwrap()
        .with_initial_url("fixture://stream-piping");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(async () => {
                const events = [];
                const sink = new WritableStream({
                    write(value) { events.push('write:' + value); },
                    close() { events.push('close'); },
                });
                const source = new ReadableStream({
                    start(controller) {
                        controller.enqueue('one');
                        controller.enqueue('two');
                        controller.close();
                    },
                });
                await source.pipeTo(sink);
                const writer = sink.getWriter();
                const writerClosed = writer.closed.then(() => 'resolved', error => 'rejected:' + error.name);
                const afterPipe = [source.locked, sink.locked, writer.desiredSize];
                await writerClosed;
                writer.releaseLock();
                const failureEvents = [];
                const failingSource = new ReadableStream({
                    start(controller) { controller.enqueue('bad'); },
                    cancel(reason) { failureEvents.push('cancel:' + reason.message); },
                });
                const failingSink = new WritableStream({
                    write() { throw new Error('sink-failure'); },
                    abort(reason) { failureEvents.push('abort:' + reason.message); },
                });
                const failure = await failingSource.pipeTo(failingSink).then(() => 'resolved', error => error.message);
                globalThis.streamPipeResult = {
                    events,
                    afterPipe,
                    writerClosed: await writerClosed,
                    failure,
                    failureEvents,
                    failingSourceLocked: failingSource.locked,
                    failingSinkLocked: failingSink.locked,
                };
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.streamPipeResult")
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["write:one", "write:two", "close"],
            "afterPipe": [false, true, null],
            "writerClosed": "resolved",
            "failure": "sink-failure",
            "failureEvents": ["abort:sink-failure", "cancel:sink-failure"],
            "failingSourceLocked": false,
            "failingSinkLocked": false
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_byte_streams_support_byob_reads_and_strategies() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://byte-streams", "<p>Byte streams</p>")
        .unwrap()
        .with_initial_url("fixture://byte-streams");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(async () => {
                const events = [];
                const source = new ReadableStream({
                    type: 'bytes',
                    pull(controller) {
                        const request = controller.byobRequest;
                        events.push(request ? ['byob', request.view.byteLength, controller.desiredSize] : ['default', controller.desiredSize]);
                        if (!request) throw new Error('BYOB request was not supplied');
                        request.view[0] = 9;
                        request.view[1] = 8;
                        request.respond(2);
                        controller.close();
                    },
                }, { highWaterMark: 2, size(chunk) { return chunk.byteLength; } });
                const reader = source.getReader({ mode: 'byob' });
                const view = new Uint8Array(4);
                const first = await reader.read(view);
                const end = await reader.read(new Uint8Array(2));
                reader.releaseLock();

                let strategyDesiredSize;
                const queued = new ReadableStream({
                    type: 'bytes',
                    start(controller) {
                        controller.enqueue(new Uint8Array([1, 2, 3]));
                        strategyDesiredSize = controller.desiredSize;
                        controller.close();
                    },
                }, { highWaterMark: 2, size(chunk) { return chunk.byteLength; } });
                const queuedReader = queued.getReader({ mode: 'byob' });
                const queuedFirst = await queuedReader.read(new Uint8Array(2));
                const queuedSecond = await queuedReader.read(new Uint8Array(2));
                const queuedEnd = await queuedReader.read(new Uint8Array(2));
                queuedReader.releaseLock();
                const replacement = new ReadableStream({
                    type: 'bytes',
                    pull(controller) {
                        controller.byobRequest.respondWithNewView(new Uint8Array([7, 6]));
                        controller.close();
                    },
                });
                const replacementReader = replacement.getReader({ mode: 'byob' });
                const replacementResult = await replacementReader.read(new Uint8Array(2));
                replacementReader.releaseLock();
                const invalid = (() => {
                    try { new ReadableStream().getReader({ mode: 'byob' }); return 'accepted'; }
                    catch (error) { return error.name; }
                })();
                globalThis.byobResult = {
                    events,
                    first: [Array.from(first.value), first.value.byteLength, first.done],
                    end: [end.value.byteLength, end.done],
                    queued: [Array.from(queuedFirst.value), Array.from(queuedSecond.value), [queuedEnd.value.byteLength, queuedEnd.done]],
                    replacement: [Array.from(replacementResult.value), replacementResult.done],
                    desiredSize: strategyDesiredSize,
                    invalid,
                };
            })()"#,
        )
        .await
        .unwrap();
    let result = engine
        .evaluate_async("globalThis.byobResult")
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "events": [["byob", 4, 2]],
            "first": [[9, 8], 2, false],
            "end": [0, true],
            "queued": [[1, 2], [3], [0, true]],
            "replacement": [[7, 6], false],
            "desiredSize": -1,
            "invalid": "TypeError"
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_raw_space_checkable_activation_is_disabled_bound() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-raw-space-checkable-disabled",
            "<input id='checkbox' type='checkbox'>",
        )
        .unwrap()
        .with_initial_url("fixture://local-raw-space-checkable-disabled");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.events = []; document.addEventListener('keydown', () => events.push('keydown')); document.addEventListener('keyup', () => events.push('keyup')); document.addEventListener('click', () => events.push('click')); document.getElementById('checkbox').focus(); true",
        )
        .await
        .unwrap();

    engine
        .action(NativeAction::KeyDown { key: " ".into() })
        .unwrap();
    engine
        .evaluate_async("document.getElementById('checkbox').disabled = true; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp { key: " ".into() })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({checked: document.getElementById('checkbox').checked, events: globalThis.events})")
            .await
            .unwrap(),
        serde_json::json!({"checked": false, "events": ["keydown", "keyup"]})
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_location_owns_navigation_and_history_replacement() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://location-start",
            "<title>Start</title><p id='part'>Start</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://location-destination",
            "<title>Destination</title><p>Arrived</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://location-replaced",
            "<title>Replaced</title><p>Rewritten</p>",
        )
        .unwrap()
        .with_initial_url("fixture://location-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "[location.href, location.origin, location.pathname, location.toString(), typeof location.assign, typeof location.replace, Object.isFrozen(location)]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "fixture://location-start",
            "null",
            "",
            "fixture://location-start",
            "function",
            "function",
            true
        ])
    );
    assert_eq!(engine.history().len(), 1);

    engine
        .evaluate_async("location.hash = 'part'")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://location-start#part"
    );
    assert_eq!(engine.history().len(), 2);

    engine
        .evaluate_async("location.assign('fixture://location-destination')")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().title, "Destination");
    assert_eq!(engine.history().len(), 3);

    engine
        .evaluate_async("location.replace('fixture://location-replaced')")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://location-replaced"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Replaced");
    assert_eq!(engine.history().len(), 3);
    assert_eq!(
        engine
            .history()
            .entries()
            .iter()
            .map(|entry| entry.url.as_str())
            .collect::<Vec<_>>(),
        vec![
            "fixture://location-start",
            "fixture://location-start#part",
            "fixture://location-replaced",
        ]
    );

    assert_eq!(
        engine.go_back().unwrap().unwrap().url,
        "fixture://location-start#part"
    );
    assert_eq!(
        engine.go_forward().unwrap().unwrap().url,
        "fixture://location-replaced"
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_show_location_replaces_publication() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://publication-start",
            "<script>addEventListener('pageshow', () => location.replace('fixture://publication-final'))</script><title>Start</title><p>Transient</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://publication-final",
            "<title>Final</title><p>Published</p>",
        )
        .unwrap()
        .with_initial_url("fixture://publication-start");
    let mut engine = NativeEngine::new(config).unwrap();

    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://publication-final"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Final");
    assert_eq!(engine.snapshot().unwrap().visible_text, "Published");
    assert_eq!(engine.history().len(), 1);
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_unhandled_rejection_dispatches_window_event() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://unhandled-rejection",
            r#"
                <script>
                    globalThis.rejectionEvents = [];
                    globalThis.handlerEvents = [];
                    globalThis.handledEvents = [];
                    globalThis.handledHandlerEvents = [];
                    window.addEventListener('unhandledrejection', event => rejectionEvents.push([
                        String(event.reason).includes('first unhandled'),
                        String(event.reason).includes('second unhandled'),
                        String(event.reason).includes('late handled'),
                        event.promise === null,
                        event instanceof PromiseRejectionEvent,
                        event.bubbles,
                        event.cancelable,
                        event.target === window,
                    ]));
                    window.onunhandledrejection = event => handlerEvents.push([
                        String(event.reason).includes('first unhandled'),
                        String(event.reason).includes('second unhandled'),
                    ]);
                    window.addEventListener('rejectionhandled', event => handledEvents.push([
                        String(event.reason).includes('late handled'),
                        event.promise === null,
                        event instanceof PromiseRejectionEvent,
                        event.bubbles,
                        event.cancelable,
                        event.target === window,
                    ]));
                    window.onrejectionhandled = event => handledHandlerEvents.push(
                        String(event.reason).includes('late handled')
                    );
                    Promise.reject('handled boom').catch(() => {});
                    Promise.reject('first unhandled');
                    Promise.reject('second unhandled');
                    globalThis.lateRejection = Promise.reject('late handled');
                    globalThis.afterRejection = true;
                </script>
                <title>Unhandled rejection is observable</title>
                <p>Document committed</p>
            "#,
        )
        .unwrap()
        .with_initial_url("fixture://unhandled-rejection");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, "fixture://unhandled-rejection");
    assert_eq!(snapshot.title, "Unhandled rejection is observable");
    assert_eq!(snapshot.visible_text, "Document committed");
    assert_eq!(
        engine
            .evaluate_async("globalThis.rejectionEvents")
            .await
            .unwrap(),
        serde_json::json!([
            [true, false, false, true, true, false, true, true],
            [false, true, false, true, true, false, true, true],
            [false, false, true, true, true, false, true, true],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.handlerEvents")
            .await
            .unwrap(),
        serde_json::json!([[true, false], [false, true], [false, false]])
    );
    assert_eq!(
        engine
            .evaluate_async("lateRejection.catch(() => {}); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.handledEvents")
            .await
            .unwrap(),
        serde_json::json!([[true, true, true, false, false, true]])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.handledHandlerEvents")
            .await
            .unwrap(),
        serde_json::json!([true])
    );
    assert_eq!(
        engine
            .evaluate_async("Boolean(globalThis.afterRejection)")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dispatches_large_promise_rejection_batch_as_data() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://large-unhandled-rejection",
            r#"
                <script>
                    globalThis.rejectionLengths = [];
                    window.addEventListener('unhandledrejection', event => {
                        rejectionLengths.push(String(event.reason).length);
                    });
                    for (let index = 0; index < 5; index++) {
                        Promise.reject('x'.repeat(4096));
                    }
                </script>
                <main>Large rejection batch</main>
            "#,
        )
        .unwrap()
        .with_initial_url("fixture://large-unhandled-rejection");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.rejectionLengths")
            .await
            .unwrap(),
        serde_json::json!([4096, 4096, 4096, 4096, 4096])
    );
    engine.close_async().await.unwrap();
}
#[test]
fn native_local_text_css_wide_resets_reach_layout_raster_and_truncation() {
    let document = NativeDocument::parse(
        "<style>.indent { display:block; width:48px; line-height:20px; } .overflow { display:block; width:40px; line-height:20px; white-space:nowrap; overflow-x:clip; } #indent-initial { text-indent:InItIaL; } #indent-unset { text-indent:UnSeT; } #indent-revert { text-indent:ReVeRt; } #indent-invalid { text-indent:16px; text-indent:1px 2px; } #indent-important { text-indent:16px !important; } #indent-important { text-indent:initial; } #overflow-initial { text-overflow:INITIAL; } #overflow-unset { text-overflow:UnSeT; } #overflow-revert { text-overflow:ReVeRt; } #overflow-invalid { text-overflow:ellipsis; text-overflow:fade; } #overflow-important { text-overflow:ellipsis !important; } #overflow-important { text-overflow:initial; }</style><div id='indent-initial' class='indent'>A</div><div id='indent-unset' class='indent'>B</div><div id='indent-revert' class='indent'>C</div><div id='indent-invalid' class='indent'>D</div><div id='indent-important' class='indent'>E</div><div id='overflow-initial' class='overflow'>ABCDEFG</div><div id='overflow-unset' class='overflow'>ABCDEFG</div><div id='overflow-revert' class='overflow'>ABCDEFG</div><div id='overflow-invalid' class='overflow'>ABCDEFG</div><div id='overflow-important' class='overflow'>ABCDEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let indent_initial = document.resolve_target("id=indent-initial").unwrap();
    let indent_unset = document.resolve_target("id=indent-unset").unwrap();
    let indent_revert = document.resolve_target("id=indent-revert").unwrap();
    let indent_invalid = document.resolve_target("id=indent-invalid").unwrap();
    let indent_important = document.resolve_target("id=indent-important").unwrap();
    let overflow_initial = document.resolve_target("id=overflow-initial").unwrap();
    let overflow_unset = document.resolve_target("id=overflow-unset").unwrap();
    let overflow_revert = document.resolve_target("id=overflow-revert").unwrap();
    let overflow_invalid = document.resolve_target("id=overflow-invalid").unwrap();
    let overflow_important = document.resolve_target("id=overflow-important").unwrap();
    let layout = document.layout(viewport).unwrap();
    let first_run = |node_id| {
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .expect("text run")
    };
    for node_id in [indent_initial, indent_unset, indent_revert] {
        assert_eq!(first_run(node_id).origin.x, 0);
    }
    assert_eq!(first_run(indent_invalid).origin.x, 16);
    assert_eq!(first_run(indent_important).origin.x, 16);
    for node_id in [overflow_initial, overflow_unset, overflow_revert] {
        let run = first_run(node_id);
        assert_eq!(run.text, "ABCDEFG");
        assert!(!run.truncated);
    }
    for node_id in [overflow_invalid, overflow_important] {
        let run = first_run(node_id);
        assert_eq!(run.text, "AB...");
        assert!(run.truncated);
    }

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, origin, .. }
                if *node_id == indent_important && origin.x == 16
        )
    }));
    let surface = list.rasterize().unwrap();
    assert!(surface.pixel(16, 1).is_some());
    let invalid_origin = first_run(indent_invalid).origin;
    assert_eq!(
        layout
            .hit_test(
                i64::from(invalid_origin.x + 1),
                i64::from(invalid_origin.y + 1),
            )
            .unwrap(),
        Some(indent_invalid)
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(diagnostic.detail.as_str(), "text-indent" | "text-overflow")
    }));
}
#[test]
fn native_local_text_revert_layer_preserves_indent_and_ellipsis_consumers() {
    let document = NativeDocument::parse(
        "<style>.indent { display:block; width:40px; line-height:20px; } .overflow { display:block; width:40px; line-height:20px; white-space:nowrap; overflow-x:clip; } @layer base { #indent { text-indent:16px; } #repeat-indent { text-indent:8px; } #inline-indent { text-indent:12px; } #fallback-indent { text-indent:revert-layer; } #invalid-indent { text-indent:16px; } #clip { text-overflow:clip; } #ellipsis { text-overflow:clip; } #repeat-overflow { text-overflow:ellipsis; } #inline-overflow { text-overflow:ellipsis; } #fallback-overflow { text-overflow:revert-layer; } #invalid-overflow { text-overflow:ellipsis; } } @layer theme { #indent { text-indent:24px; } #repeat-indent { text-indent:20px; } #ellipsis { text-overflow:ellipsis; } #repeat-overflow { text-overflow:clip; } } @layer top { #repeat-indent { text-indent:revert-layer; } #repeat-overflow { text-overflow:revert-layer; } } #indent { text-indent:revert-layer; } #repeat-indent { text-indent:revert-layer; } #clip { text-overflow:revert-layer; } #ellipsis { text-overflow:revert-layer; } #repeat-overflow { text-overflow:revert-layer; } #invalid-indent { text-indent:1px 2px; } #invalid-overflow { text-overflow:fade; }</style><div id='indent' class='indent'>ABCDEFGH</div><div id='repeat-indent' class='indent'>ABCDEFGH</div><div id='inline-indent' class='indent' style='text-indent:revert-layer'>ABCDEFGH</div><div id='fallback-indent' class='indent'>ABCDEFGH</div><div id='invalid-indent' class='indent'>ABCDEFGH</div><div id='clip' class='overflow'>ABCDEFG</div><div id='ellipsis' class='overflow'>ABCDEFG</div><div id='repeat-overflow' class='overflow'>ABCDEFG</div><div id='inline-overflow' class='overflow' style='text-overflow:revert-layer'>ABCDEFG</div><div id='fallback-overflow' class='overflow'>ABCDEFG</div><div id='invalid-overflow' class='overflow'>ABCDEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let indent = document.resolve_target("id=indent").unwrap();
    let repeat_indent = document.resolve_target("id=repeat-indent").unwrap();
    let inline_indent = document.resolve_target("id=inline-indent").unwrap();
    let fallback_indent = document.resolve_target("id=fallback-indent").unwrap();
    let invalid_indent = document.resolve_target("id=invalid-indent").unwrap();
    let clip = document.resolve_target("id=clip").unwrap();
    let ellipsis = document.resolve_target("id=ellipsis").unwrap();
    let repeat_overflow = document.resolve_target("id=repeat-overflow").unwrap();
    let inline_overflow = document.resolve_target("id=inline-overflow").unwrap();
    let fallback_overflow = document.resolve_target("id=fallback-overflow").unwrap();
    let invalid_overflow = document.resolve_target("id=invalid-overflow").unwrap();
    let layout = document.layout(viewport).unwrap();

    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>()
    };
    assert_eq!(runs_for(indent)[0].0.x, 24);
    assert_eq!(runs_for(repeat_indent)[0].0.x, 20);
    assert_eq!(runs_for(inline_indent)[0].0.x, 12);
    assert_eq!(runs_for(fallback_indent)[0].0.x, 0);
    assert_eq!(runs_for(invalid_indent)[0].0.x, 16);

    assert_eq!(
        runs_for(clip),
        vec![(NativePoint { x: 0, y: 240 }, "ABCDEFG", false)]
    );
    assert_eq!(
        runs_for(ellipsis),
        vec![(NativePoint { x: 0, y: 260 }, "AB...", true)]
    );
    assert_eq!(
        runs_for(repeat_overflow),
        vec![(NativePoint { x: 0, y: 280 }, "ABCDEFG", false)]
    );
    assert_eq!(
        runs_for(inline_overflow),
        vec![(NativePoint { x: 0, y: 300 }, "AB...", true)]
    );
    assert_eq!(
        runs_for(fallback_overflow),
        vec![(NativePoint { x: 0, y: 320 }, "ABCDEFG", false)]
    );
    assert_eq!(
        runs_for(invalid_overflow),
        vec![(NativePoint { x: 0, y: 340 }, "AB...", true)]
    );

    let list = document.display_list(viewport).unwrap();
    for run in &layout.text_runs {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::TextRun { node_id, origin, text, truncated, .. }
                    if *node_id == run.node_id
                        && *origin == run.origin
                        && text == &run.text
                        && *truncated == run.truncated
            )
        }));
    }
    assert_eq!(layout.hit_test(25, 1).unwrap(), Some(indent));
}
#[test]
fn native_local_dimension_revert_layer_preserves_box_geometry_and_hit_testing() {
    let document = NativeDocument::parse(
        "<style>.box { display:block; margin:0px; padding:0; } @layer base { #named { width:24px; height:12px; min-width:4px; max-width:80px; min-height:6px; max-height:90px; background-color:red; } #repeat { width:20px; height:10px; min-width:2px; max-width:70px; min-height:4px; max-height:60px; background-color:blue; } #fallback { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #invalid { width:30px; height:18px; min-width:8px; max-width:70px; min-height:10px; max-height:40px; } } @layer theme { #named { width:48px; height:20px; min-width:16px; max-width:100px; min-height:10px; max-height:110px; } #repeat { width:36px; height:16px; min-width:12px; max-width:90px; min-height:8px; max-height:80px; } } @layer top { #repeat { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } } #named { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #repeat { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #invalid { width:1px 2px; height:1px 2px; min-width:1px 2px; max-width:1px 2px; min-height:1px 2px; max-height:50%; }</style><div id='named' class='box'>Named</div><div id='repeat' class='box'>Repeat</div><div id='fallback' class='box'>Fallback</div><div id='invalid' class='box'>Invalid</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 120,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let named = document.resolve_target("id=named").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(named),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 48,
            height: 20,
        })
    );
    assert_eq!(
        layout.box_for(repeat),
        Some(NativeRect {
            x: 0,
            y: 20,
            width: 36,
            height: 16,
        })
    );
    assert_eq!(
        layout.box_for(fallback),
        Some(NativeRect {
            x: 0,
            y: 36,
            width: 120,
            height: 20,
        })
    );
    assert_eq!(
        layout.box_for(invalid),
        Some(NativeRect {
            x: 0,
            y: 56,
            width: 30,
            height: 18,
        })
    );
    assert_eq!(layout.hit_test(47, 1).unwrap(), Some(named));
    assert_eq!(layout.hit_test(35, 21).unwrap(), Some(repeat));

    let surface = document
        .display_list(viewport)
        .unwrap()
        .rasterize()
        .unwrap();
    assert_eq!(surface.pixel(47, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(35, 21), Some([0, 0, 255, 255]));
}
#[tokio::test]
async fn native_local_url_attributes_resolve_and_persist_through_dom_commands() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://url-properties/path/page")
        .with_fixture(
            "fixture://url-properties/path/page",
            "<a id='link' href='../next?tab=one#part'>next</a><img id='image' src='assets/pixel.png'><form id='form' action='/submit'><input></form><script id='script' src='../scripts/app.js'></script><link id='stylesheet' rel='stylesheet' href='../styles/main.css'>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const link = document.getElementById('link'); const image = document.getElementById('image'); const form = document.getElementById('form'); const script = document.getElementById('script'); const stylesheet = document.getElementById('stylesheet'); return [link.href, image.src, form.action, script.src, stylesheet.href, link.getAttribute('href'), image.getAttribute('src')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "fixture://url-properties/next?tab=one#part",
            "fixture://url-properties/path/assets/pixel.png",
            "fixture://url-properties/submit",
            "fixture://url-properties/scripts/app.js",
            "fixture://url-properties/styles/main.css",
            "../next?tab=one#part",
            "assets/pixel.png",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); image.src = '../images/next.png'; return [image.src, image.getAttribute('src')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "fixture://url-properties/images/next.png",
            "../images/next.png",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("[document.getElementById('image').src, document.getElementById('image').getAttribute('src')]")
            .await
            .unwrap(),
        serde_json::json!([
            "fixture://url-properties/images/next.png",
            "../images/next.png",
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_url_resolution_uses_canonical_url_parser() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://url-parser/index.html", "<p>URL parser</p>")
        .unwrap()
        .with_initial_url("fixture://url-parser/index.html");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const spaced = new URL('http://f:80/ b ? d # e ');
                    const defaultPort = new URL('https://Example.COM:443/');
                    const schemeRelative = new URL('//Example.COM:443/path', 'https://base.test/root');
                    const absoluteSpecial = new URL('http:example.com/');
                    const baseSpecial = new URL('http:example.com/', 'http://example.org/foo/bar');
                    const opaque = new URL('data:text/plain,hello world');
                    const mutable = new URL('http://example.test/base/start?x=1');
                    mutable.pathname = '/changed/../new path';
                    mutable.hash = 'part with space';
                    let invalid = '';
                    try { new URL('http://bad host/'); } catch (error) { invalid = error.name; }
                    return {
                        spaced: [spaced.href, spaced.origin, spaced.pathname, spaced.search, spaced.hash],
                        defaultPort: [defaultPort.href, defaultPort.host, defaultPort.port, defaultPort.origin],
                        schemeRelative: [schemeRelative.href, schemeRelative.origin],
                        special: [absoluteSpecial.href, baseSpecial.href],
                        opaque: [opaque.href, opaque.origin, opaque.pathname],
                        mutable: [mutable.href, mutable.pathname, mutable.search, mutable.hash],
                        invalid,
                    };
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "spaced": [
                "http://f/%20b%20?%20d%20#%20e",
                "http://f",
                "/%20b%20",
                "?%20d%20",
                "#%20e",
            ],
            "defaultPort": [
                "https://example.com/",
                "example.com",
                "",
                "https://example.com",
            ],
            "schemeRelative": ["https://example.com/path", "https://example.com"],
            "special": [
                "http://example.com/",
                "http://example.org/foo/example.com/",
            ],
            "opaque": ["data:text/plain,hello world", "null", "text/plain,hello world"],
            "mutable": [
                "http://example.test/new%20path?x=1#part%20with%20space",
                "/new%20path",
                "?x=1",
                "#part%20with%20space",
            ],
            "invalid": "TypeError",
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_namespace_dom_preserves_svg_mathml_and_foreign_content() {
    let mut engine = NativeEngine::new(NativeEngineConfig::default().with_initial_url(
        "data:text/html,%3Cbody%3E%3Csvg%20id%3D%27parsed-svg%27%3E%3Crect%20id%3D%27parsed-rect%27%3E%3C%2Frect%3E%3CforeignObject%20id%3D%27foreign%27%3E%3Cdiv%20id%3D%27foreign-html%27%3Econtent%3C%2Fdiv%3E%3C%2FforeignObject%3E%3C%2Fsvg%3E%3C%2Fbody%3E",
    ))
    .unwrap();
    engine.initialize_async().await.unwrap();

    let result = engine
        .evaluate_async(
            "(() => { const svg = document.getElementById('parsed-svg'); const rect = document.getElementById('parsed-rect'); const foreign = document.getElementById('foreign'); const foreignHtml = document.getElementById('foreign-html'); const created = document.createElementNS('http://www.w3.org/2000/svg', 'circle'); created.id = 'created-circle'; created.setAttribute('cx', '4'); svg.appendChild(created); const math = document.createElementNS('http://www.w3.org/1998/Math/MathML', 'mi'); math.textContent = 'x'; math.id = 'created-math'; document.body.appendChild(math); const nullNamespace = document.createElementNS(null, 'custom-node'); const html = document.createElement('div'); html.innerHTML = '<svg><circle id=inner-circle></circle></svg>'; document.body.appendChild(html); let invalidName = ''; try { document.createElementNS('https://example.com/not-supported', 'x'); } catch (error) { invalidName = error.name; } return { parsed: [svg.namespaceURI, rect.namespaceURI, foreign.namespaceURI, foreignHtml.namespaceURI], created: [created.namespaceURI, created.localName, created.tagName, document.getElementById('created-circle') === created], math: [math.namespaceURI, math.localName, math.tagName], nullNamespace: nullNamespace.namespaceURI, parser: [html.firstElementChild.namespaceURI, html.firstElementChild.firstElementChild.namespaceURI], invalidName }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "parsed": [
                "http://www.w3.org/2000/svg",
                "http://www.w3.org/2000/svg",
                "http://www.w3.org/2000/svg",
                "http://www.w3.org/1999/xhtml",
            ],
            "created": ["http://www.w3.org/2000/svg", "circle", "circle", true],
            "math": ["http://www.w3.org/1998/Math/MathML", "mi", "mi"],
            "nullNamespace": null,
            "parser": [
                "http://www.w3.org/2000/svg",
                "http://www.w3.org/2000/svg",
            ],
            "invalidName": "NamespaceError",
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_runtime_preserves_custom_element_registry_and_builtins() {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://inline-custom-elements",
                "<!doctype html><html><body><x-inline-probe id='probe' data-state='parser'></x-inline-probe><button id='parsed-button' is='x-inline-button'>Go</button></body></html>",
            )
            .unwrap()
            .with_initial_url("fixture://inline-custom-elements"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r##"await (async () => {
                  const trace = [];
                  class InlineProbe extends HTMLElement {
                    static get observedAttributes() { return ['data-state']; }
                    constructor() { super(); trace.push('construct:' + this.id); }
                    attributeChangedCallback(name, _oldValue, value) {
                      trace.push('attribute:' + name + ':' + value);
                    }
                    connectedCallback() { trace.push('connected:' + this.id); }
                  }
                  customElements.define('x-inline-probe', InlineProbe);

                  class InlineButton extends HTMLButtonElement {
                    constructor() { super(); trace.push('construct:button'); }
                  }
                  customElements.define('x-inline-button', InlineButton, { extends: 'button' });

                  const parsedProbe = document.getElementById('probe');
                  const parsedButton = document.getElementById('parsed-button');
                  const createdButton = document.createElement('button', { is: 'x-inline-button' });
                  let clickCount = 0;
                  createdButton.addEventListener('click', () => { clickCount += 1; });
                  document.body.appendChild(createdButton);
                  createdButton.click();
                  return {
                    autonomous: [parsedProbe instanceof InlineProbe, parsedProbe.localName],
                    buttons: [
                      parsedButton instanceof InlineButton,
                      parsedButton instanceof HTMLButtonElement,
                      parsedButton.getAttribute('is'),
                      createdButton instanceof InlineButton,
                      createdButton instanceof HTMLButtonElement,
                      createdButton.getAttribute('is'),
                      createdButton.outerHTML.includes('is=\"x-inline-button\"'),
                    ],
                    clickCount,
                    trace,
                  };
                })()"##,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "autonomous": [true, "x-inline-probe"],
            "buttons": [
                true,
                true,
                "x-inline-button",
                true,
                true,
                null,
                true,
            ],
            "clickCount": 1,
            "trace": [
                "construct:probe",
                "attribute:data-state:parser",
                "connected:probe",
                "construct:button",
                "construct:button",
            ],
        })
    );

    engine.close_async().await.unwrap();
}
