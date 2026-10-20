use super::*;

#[tokio::test]
async fn native_content_process_xhr_supports_json_response_type_in_page_and_worker() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let (content_type, body) = match path {
                "/page" => (
                    "text/html",
                    r#"<script>
globalThis.pageJsonPromise = new Promise(resolve => {
  const xhr = new XMLHttpRequest();
  xhr.responseType = 'json';
  xhr.onload = () => {
    let textError = '';
    try { xhr.responseText; } catch (error) { textError = error.name; }
    resolve({ identity: xhr.response instanceof Object, data: xhr.response, text: textError });
  };
  xhr.onerror = () => resolve({ error: 'page-xhr' });
  xhr.open('GET', '/json');
  xhr.send();
});
globalThis.workerMessages = [];
globalThis.workerPromise = new Promise(resolve => {
  globalThis.worker = new Worker('/worker.js');
  worker.onmessage = event => { workerMessages.push(event.data); resolve(event.data); };
});
</script><main>XHR JSON</main>"#,
                ),
                "/worker.js" => (
                    "text/javascript",
                    r#"const xhr = new XMLHttpRequest();
xhr.responseType = 'json';
xhr.onload = () => {
  let textError = '';
  try { xhr.responseText; } catch (error) { textError = error.name; }
  postMessage({ identity: xhr.response instanceof Object, data: xhr.response, text: textError });
};
xhr.onerror = () => postMessage({ error: 'worker-xhr' });
xhr.open('GET', '/worker-json');
xhr.send();"#,
                ),
                "/json" => ("application/json", r#"{"page":true,"value":7}"#),
                "/worker-json" => ("application/json", r#"{"worker":true,"value":9}"#),
                other => panic!("unexpected XHR JSON request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("await Promise.all([pageJsonPromise, workerPromise])")
            .await
            .unwrap(),
        serde_json::json!([
            {
                "identity": true,
                "data": {"page": true, "value": 7},
                "text": "InvalidStateError"
            },
            {
                "identity": true,
                "data": {"worker": true, "value": 9},
                "text": "InvalidStateError"
            }
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_exposes_bounded_xml_response_document() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/xml-default", "/xml-document", "/xml-invalid"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for XML XHR request {expected_path}"))
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(
                request.split_whitespace().nth(1),
                Some(expected_path),
                "unexpected request while awaiting {expected_path}: {request}"
            );
            let (content_type, body) = match expected_path {
                "/page" => ("text/html", "<p>XHR XML</p>"),
                "/xml-default" | "/xml-document" => (
                    "application/xml; charset=utf-8",
                    r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE catalog SYSTEM "catalog.dtd"><catalog xmlns="urn:glass" xmlns:g="urn:glass:extra" id="root"><!--comment--><g:item id="item-1" g:kind="primary"><![CDATA[raw <value>]]></g:item><?glass done?></catalog>"#,
                ),
                "/xml-invalid" => ("application/xml", "<catalog><item></catalog>"),
                other => panic!("unexpected XHR XML request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                globalThis.xmlResultPromise = new Promise(resolve => {
                    const result = {};
                    const fail = (stage, error) => resolve({
                        error: stage,
                        message: String(error && error.message || error),
                    });
                    const invalid = new XMLHttpRequest();
                    invalid.open('GET', '/xml-invalid');
                    invalid.onload = () => {
                        try {
                            result.invalid = {
                                responseXML: invalid.responseXML,
                                responseText: invalid.responseText,
                            };
                            const unsupported = new XMLHttpRequest();
                            unsupported.responseType = 'json';
                            try {
                                unsupported.responseXML;
                                result.unsupported = 'no-error';
                            } catch (error) {
                                result.unsupported = error.name;
                            }
                            resolve(result);
                        } catch (error) {
                            fail('invalid-callback', error);
                        }
                    };
                    invalid.onerror = error => fail('invalid-xhr', error);

                    const explicit = new XMLHttpRequest();
                    explicit.responseType = 'document';
                    explicit.open('GET', '/xml-document');
                    explicit.onload = () => {
                        try {
                            const document = explicit.response;
                            let responseTextError = '';
                            try { explicit.responseText; } catch (error) { responseTextError = error.name; }
                            result.explicit = {
                                identity: document instanceof Document && document === explicit.responseXML,
                                responseTextError,
                                root: document.documentElement.nodeName,
                                namespace: document.documentElement.namespaceURI,
                            };
                            invalid.send();
                        } catch (error) {
                            fail('explicit-callback', error);
                        }
                    };
                    explicit.onerror = error => fail('explicit-xhr', error);

                    const request = new XMLHttpRequest();
                    request.open('GET', '/xml-default');
                    request.onload = () => {
                        try {
                            const document = request.responseXML;
                            const root = document.documentElement;
                            const item = document.getElementById('item-1');
                            let appendError = '';
                            try { root.appendChild(item); } catch (error) { appendError = error.name; }
                            const serialized = new XMLSerializer().serializeToString(document);
                            result.default = {
                                document: document instanceof Document,
                                responseIdentity: request.response === request.responseText,
                                responseXmlIdentity: document === request.responseXML,
                                root: root.nodeName,
                                rootNamespace: root.namespaceURI,
                                itemNamespace: item.namespaceURI,
                                itemPrefix: item.prefix,
                                itemText: item.textContent,
                                itemParent: item.parentNode === root && item.parentElement === root,
                                itemId: item.getAttribute('id'),
                                itemKind: item.getAttributeNS('urn:glass:extra', 'kind'),
                                namespaceAttribute: root.getAttributeNS('http://www.w3.org/2000/xmlns/', 'g'),
                                tagMatches: document.getElementsByTagName('g:item').length,
                                namespaceMatches: document.getElementsByTagNameNS('urn:glass:extra', 'item').length,
                                childCount: root.childNodes.length,
                                elementChildCount: root.children.length,
                                comment: root.childNodes[0].nodeType === 8 && root.childNodes[0].nodeValue === 'comment',
                                cdata: item.firstChild.nodeType === 4 && item.firstChild.nodeValue === 'raw <value>',
                                processingInstruction: root.lastChild.nodeType === 7 && root.lastChild.target === 'glass' && root.lastChild.data === 'done',
                                doctype: document.doctype.name === 'catalog' && document.doctype.systemId === 'catalog.dtd',
                                serialized: serialized.includes('<!DOCTYPE catalog SYSTEM "catalog.dtd">')
                                    && serialized.includes('<![CDATA[raw <value>]]>')
                                    && serialized.includes('g:kind="primary"'),
                                immutable: appendError === 'NoModificationAllowedError' && Object.isFrozen(document) && Object.isFrozen(root),
                            };
                            explicit.send();
                        } catch (error) {
                            fail('default-callback', error);
                        }
                    };
                    request.onerror = error => fail('default-xhr', error);
                    request.send();
                });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await xmlResultPromise")
            .await
            .unwrap(),
        serde_json::json!({
            "default": {
                "document": true,
                "responseIdentity": true,
                "responseXmlIdentity": true,
                "root": "catalog",
                "rootNamespace": "urn:glass",
                "itemNamespace": "urn:glass:extra",
                "itemPrefix": "g",
                "itemText": "raw <value>",
                "itemParent": true,
                "itemId": "item-1",
                "itemKind": "primary",
                "namespaceAttribute": "urn:glass:extra",
                "tagMatches": 1,
                "namespaceMatches": 1,
                "childCount": 3,
                "elementChildCount": 1,
                "comment": true,
                "cdata": true,
                "processingInstruction": true,
                "doctype": true,
                "serialized": true,
                "immutable": true,
            },
            "explicit": {
                "identity": true,
                "responseTextError": "InvalidStateError",
                "root": "catalog",
                "namespace": "urn:glass",
            },
            "invalid": {
                "responseXML": null,
                "responseText": "<catalog><item></catalog>",
            },
            "unsupported": "InvalidStateError",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_response_type_is_canonical_and_state_aware() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut seen_paths = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for the next response-type request; accepted paths: {seen_paths:?}"))
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            seen_paths.push(path.to_owned());
            let (content_type, body) = match path {
                "/page" => (
                    "text/html",
                    r#"<script>
globalThis.pageResponseTypePromise = new Promise(resolve => {
  const xhr = new XMLHttpRequest();
  let invalidType = '';
  let loadingError = '';
  let doneError = '';
  let credentialsDoneError = '';
  let forbiddenMethod = '';
  let invalidMethod = '';
  let customMethod = '';
  const methodProbe = new XMLHttpRequest();
  try { methodProbe.open('TRACE', 'unused'); } catch (error) { forbiddenMethod = error.name; }
  try { methodProbe.open('GET\n', 'unused'); } catch (error) { invalidMethod = error.name; }
  const customMethodProbe = new XMLHttpRequest();
  try { customMethodProbe.open('rEpOrT', 'unused'); customMethod = customMethodProbe._method; } catch (error) { customMethod = error.name; }
  const credentials = new XMLHttpRequest();
  credentials.withCredentials = 'false';
  const credentialsBeforeOpen = credentials.withCredentials;
  credentials.open('GET', '/unused');
  const credentialsAfterOpen = credentials.withCredentials;
  credentials.withCredentials = 0;
  const credentialsAfterSet = credentials.withCredentials;
  const responseTypeSync = new XMLHttpRequest();
  responseTypeSync.responseType = 'json';
  let syncOpenError = '';
  try { responseTypeSync.open('GET', '/unused', false); } catch (error) { syncOpenError = error.name; }
  const sync = new XMLHttpRequest();
  sync.open('GET', '/unused', 0);
  let syncTimeoutError = '';
  let syncResponseTypeError = '';
  try { sync.timeout = 1; } catch (error) { syncTimeoutError = error.name; }
  try { sync.responseType = 'json'; } catch (error) { syncResponseTypeError = error.name; }
  xhr.open('GET', 'type-page');
  const urlObjectProbe = new XMLHttpRequest();
  urlObjectProbe.open('GET', new URL('type-page', location.href));
  const urlObjectState = urlObjectProbe.readyState;
  const credentialProbe = new XMLHttpRequest();
  credentialProbe.open('GET', new URL('type-page', location.href), true, 'user name', 'p@ss');
  const credentialUrl = credentialProbe._url;
  try { xhr.responseType = 'unsupported'; } catch (error) { invalidType = error.name; }
  xhr.responseType = 'JSON';
  xhr.onreadystatechange = () => {
    if (xhr.readyState === 3) {
      try { xhr.responseType = 'text'; } catch (error) { loadingError = error.name; }
    }
    if (xhr.readyState === 4) {
      try { xhr.responseType = 'text'; } catch (error) { doneError = error.name; }
    }
  };
  xhr.onload = () => {
    try { xhr.withCredentials = true; } catch (error) { credentialsDoneError = error.name; }
    resolve({
      canonical: xhr.responseType,
      data: xhr.response,
      invalidType,
      loadingError,
      doneError,
      credentials: [credentialsBeforeOpen, credentialsAfterOpen, credentialsAfterSet],
      credentialsDoneError,
      open: [forbiddenMethod, invalidMethod, customMethod, urlObjectState, credentialUrl.includes('user%20name:p%40ss@')],
      sync: [syncOpenError, syncTimeoutError, syncResponseTypeError],
    });
  };
  xhr.onerror = () => resolve({ error: 'page-xhr' });
  xhr.send();
});
globalThis.workerResponseTypePromise = new Promise(resolve => {
  const worker = new Worker('/worker-type.js');
  worker.onmessage = event => resolve(event.data);
  worker.onerror = () => resolve({ error: 'worker-script' });
});
</script><main>XHR response type</main>"#,
                ),
                "/worker-type.js" => (
                    "text/javascript",
                    r#"const xhr = new XMLHttpRequest();
let invalidType = '';
let loadingError = '';
let doneError = '';
let credentialsDoneError = '';
let forbiddenMethod = '';
let invalidMethod = '';
let customMethod = '';
const methodProbe = new XMLHttpRequest();
try { methodProbe.open('TRACE', 'unused'); } catch (error) { forbiddenMethod = error.name; }
try { methodProbe.open('GET\n', 'unused'); } catch (error) { invalidMethod = error.name; }
const customMethodProbe = new XMLHttpRequest();
try { customMethodProbe.open('rEpOrT', 'unused'); customMethod = customMethodProbe._method; } catch (error) { customMethod = error.name; }
const credentials = new XMLHttpRequest();
credentials.withCredentials = 'false';
const credentialsBeforeOpen = credentials.withCredentials;
credentials.open('GET', '/unused');
const credentialsAfterOpen = credentials.withCredentials;
credentials.withCredentials = 0;
const credentialsAfterSet = credentials.withCredentials;
const sync = new XMLHttpRequest();
sync.open('GET', 'type-worker', false);
sync.timeout = 10;
sync.responseType = 'json';
xhr.open('GET', 'type-worker');
const urlObjectProbe = new XMLHttpRequest();
urlObjectProbe.open('GET', new URL('type-worker', location.href));
const urlObjectState = urlObjectProbe.readyState;
const credentialProbe = new XMLHttpRequest();
credentialProbe.open('GET', new URL('type-worker', location.href), true, 'worker name', 'p@ss');
const credentialUrl = credentialProbe._url;
try { xhr.responseType = 'unsupported'; } catch (error) { invalidType = error.name; }
xhr.responseType = 'DOCUMENT';
const documentIgnored = xhr.responseType;
xhr.responseType = 'TEXT';
xhr.onreadystatechange = () => {
  if (xhr.readyState === 3) {
    try { xhr.responseType = 'json'; } catch (error) { loadingError = error.name; }
  }
  if (xhr.readyState === 4) {
    try { xhr.responseType = 'json'; } catch (error) { doneError = error.name; }
  }
};
xhr.onload = () => {
  try { xhr.withCredentials = true; } catch (error) { credentialsDoneError = error.name; }
  postMessage({
    canonical: xhr.responseType,
    data: xhr.response,
    invalidType,
    documentIgnored,
    loadingError,
    doneError,
    credentials: [credentialsBeforeOpen, credentialsAfterOpen, credentialsAfterSet],
    credentialsDoneError,
    open: [forbiddenMethod, invalidMethod, customMethod, urlObjectState, credentialUrl.includes('worker%20name:p%40ss@')],
    sync: [sync.timeout, sync.responseType],
  });
};
xhr.onerror = () => postMessage({ error: 'worker-xhr' });
xhr.send();"#,
                ),
                "/type-page" => ("application/json", r#"{"page":true}"#),
                "/type-worker" => ("text/plain", "worker-response"),
                other => panic!("unexpected response-type request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_context_id(format!("xhr-response-type-{}", std::process::id()))
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await Promise.all([pageResponseTypePromise, workerResponseTypePromise])",
            )
            .await
            .unwrap(),
        serde_json::json!([
            {
                "canonical": "json",
                "data": {"page": true},
                "invalidType": "TypeError",
                "loadingError": "InvalidStateError",
                "doneError": "InvalidStateError",
                "credentials": [true, true, false],
                "credentialsDoneError": "InvalidStateError",
                "open": ["SecurityError", "SyntaxError", "REPORT", 1, true],
                "sync": ["InvalidAccessError", "InvalidAccessError", "InvalidAccessError"],
            },
            {
                "canonical": "text",
                "data": "worker-response",
                "invalidType": "TypeError",
                "documentIgnored": "",
                "loadingError": "InvalidStateError",
                "doneError": "InvalidStateError",
                "credentials": [true, true, false],
                "credentialsDoneError": "InvalidStateError",
                "open": ["SecurityError", "SyntaxError", "REPORT", 1, true],
                "sync": [10, "json"],
            },
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_exposes_bounded_html_response_document() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_script = r###"(() => {
                globalThis.htmlResultPromise = new Promise(resolve => {
                    const result = {};
                    const fail = (stage, error, xhr = null) => resolve({
                        error: stage,
                        message: String(error && error.error && (
                            error.error.message || error.error
                        ) || error && error.message || error),
                        location: location.href,
                        xhr: xhr && {
                            readyState: xhr.readyState,
                            responseType: xhr.responseType,
                            responseURL: xhr.responseURL,
                            status: xhr.status,
                            statusText: xhr.statusText,
                        },
                    });
                    const invalid = new XMLHttpRequest();
                    invalid.responseType = 'DOCUMENT';
                    invalid.open('GET', '/html-invalid');
                    invalid.onload = () => {
                        try {
                            let responseTextError = '';
                            try { invalid.responseText; } catch (error) { responseTextError = error.name; }
                            const response = invalid.response;
                            result.invalid = {
                                identity: response instanceof Document
                                    && response === invalid.responseXML,
                                documentElement: response && response.documentElement.localName,
                                recoveredComment: response && response.body.lastChild.nodeType === 8
                                    && response.body.lastChild.nodeValue === 'unterminated',
                                compatMode: response && response.compatMode,
                                responseTextError,
                            };
                            const charsetRequest = new XMLHttpRequest();
                            charsetRequest.responseType = 'document';
                            charsetRequest.open('GET', '/html-windows-1252');
                            charsetRequest.onload = () => {
                                try {
                                    result.charset = {
                                        title: charsetRequest.response.title,
                                        body: charsetRequest.response.body.textContent,
                                    };
                                    const overrideRequest = new XMLHttpRequest();
                                    overrideRequest.responseType = 'document';
                                    overrideRequest.overrideMimeType(
                                        'text/html; charset=windows-1252',
                                    );
                                    overrideRequest.open('GET', '/html-override-charset');
                                    overrideRequest.onload = () => {
                                        try {
                                            result.overrideCharset = {
                                                title: overrideRequest.response.title,
                                                body: overrideRequest.response.body.textContent,
                                            };
                                            const utf16Request = new XMLHttpRequest();
                                            utf16Request.responseType = 'document';
                                            utf16Request.open('GET', '/html-utf16-bom');
                                            utf16Request.onload = () => {
                                                (async () => {
                                                    result.utf16Bom = utf16Request.response.title;
                                                    const readDocumentTitle = path => new Promise((resolveTitle, rejectTitle) => {
                                                        const request = new XMLHttpRequest();
                                                        request.responseType = 'document';
                                                        request.open('GET', path);
                                                        request.onload = () => resolveTitle(request.response.title);
                                                        request.onerror = () => rejectTitle(new Error('XHR failed: ' + path));
                                                        request.send();
                                                    });
                                                    result.headerShiftJis = await readDocumentTitle('/html-header-shift-jis');
                                                    result.metaCharset = await readDocumentTitle('/html-meta-shift-jis');
                                                    result.metaContent = await readDocumentTitle('/html-meta-content-gbk');
                                                    resolve(result);
                                                })().catch(error => {
                                                    fail('utf16-callback', error);
                                                });
                                            };
                                            utf16Request.onerror = error =>
                                                fail('utf16-xhr', error, utf16Request);
                                            utf16Request.send();
                                        } catch (error) {
                                            fail('override-charset-callback', error);
                                        }
                                    };
                                    overrideRequest.onerror = error =>
                                        fail('override-charset-xhr', error, overrideRequest);
                                    overrideRequest.send();
                                } catch (error) {
                                    fail('charset-callback', error);
                                }
                            };
                            charsetRequest.onerror = error =>
                                fail('charset-xhr', error, charsetRequest);
                            charsetRequest.send();
                        } catch (error) {
                            fail('invalid-callback', error);
                        }
                    };
                    invalid.onerror = error => fail('invalid-xhr', error, invalid);

                    const defaultRequest = new XMLHttpRequest();
                    defaultRequest.open('GET', '/html-default');
                    defaultRequest.onload = () => {
                        try {
                            result.default = {
                                response: defaultRequest.response,
                                responseText: defaultRequest.responseText,
                                responseXml: defaultRequest.responseXML,
                            };
                            invalid.send();
                        } catch (error) {
                            fail('default-callback', error);
                        }
                    };
                    defaultRequest.onerror = error => fail('default-xhr', error, defaultRequest);

                    const explicit = new XMLHttpRequest();
                    explicit.responseType = 'DOCUMENT';
                    explicit.open('GET', '/html-document');
                    explicit.onload = () => {
                        try {
                            const document = explicit.response;
                            let responseTextError = '';
                            try { explicit.responseText; } catch (error) { responseTextError = error.name; }
                            const root = document.documentElement;
                            const head = document.head;
                            const body = document.body;
                            const first = document.getElementById('first');
                            const second = document.querySelector('#second');
                            const script = document.getElementById('raw');
                            const detachedImage = document.getElementById('detached-image');
                            const textarea = document.getElementById('rc');
                            let appendError = '';
                            try { root.appendChild(first); } catch (error) { appendError = error.name; }
                            const serialized = new XMLSerializer().serializeToString(document);
                            result.explicit = {
                                identity: document instanceof Document
                                    && document === explicit.responseXML,
                                contentType: document.contentType,
                                responseUrl: document.URL === explicit.responseURL
                                    && document.documentURI === explicit.responseURL,
                                responseTextError,
                                root: [root.nodeName, root.localName, root.tagName],
                                title: document.title,
                                head: head.nodeName,
                                body: body.nodeName,
                                bodyText: body.textContent.includes('one')
                                    && body.textContent.includes('A & B'),
                                scriptText: script.textContent,
                                inertScript: globalThis.__glassXhrProcessScriptRan === undefined,
                                detachedImage: detachedImage.getAttribute('src'),
                                textareaText: textarea.textContent,
                                firstAttribute: first.getAttribute('DATA-VALUE'),
                                firstParent: first.parentNode === body
                                    && first.ownerDocument === document,
                                secondIdentity: second === document.getElementsByTagName('DIV')[0],
                                tagMatches: document.getElementsByTagName('div').length,
                                namespaceMatches: document.getElementsByTagNameNS(
                                    'http://www.w3.org/1999/xhtml', 'DIV',
                                ).length,
                                voids: body.querySelectorAll('br').length === 1
                                    && body.querySelector('input').hasAttribute('DISABLED'),
                                doctype: document.doctype.name,
                                compatMode: document.compatMode,
                                serialized: serialized.includes('<!DOCTYPE html>')
                                    && serialized.includes('<br>')
                                    && serialized.includes('if (a < b) rawValue();')
                                    && !serialized.includes('<br/>'),
                                immutable: appendError === 'NoModificationAllowedError'
                                    && Object.isFrozen(document)
                                    && Object.isFrozen(root),
                            };
                            defaultRequest.send();
                        } catch (error) {
                            fail('explicit-callback', error);
                        }
                    };
                    explicit.onerror = error => fail('explicit-xhr', error, explicit);
                    explicit.send();
                });
            })()"###;
    let template_page_script = r###"globalThis.htmlResultPromise = globalThis.htmlResultPromise.then(result => new Promise((resolveTemplate, rejectTemplate) => {
  const request = new XMLHttpRequest();
  request.responseType = 'document';
  request.open('GET', '/html-template');
  request.onload = () => {
    try {
      const response = request.response;
      const template = response.getElementById('template');
      const content = template.content;
      const cell = content.getElementById('template-cell');
      const nested = content.querySelector('#nested-template');
      let appendError = '';
      try { content.appendChild(cell); } catch (error) { appendError = error.name; }
      const serializer = new XMLSerializer();
      result.template = {
        templateType: template instanceof HTMLTemplateElement
          && template instanceof HTMLElement,
        contentIdentity: content === template.content
          && content instanceof DocumentFragment
          && content.nodeType === 11
          && content.nodeName === '#document-fragment',
        ownerDocument: content.ownerDocument !== response
          && content.ownerDocument.nodeType === 9
          && content.ownerDocument.contentType === 'text/html'
          && nested.content.ownerDocument === content.ownerDocument,
        parents: content.parentNode === null
          && template.childNodes.length === 0
          && template.children.length === 0
          && cell.getRootNode() === content
          && cell.parentElement.localName === 'tr',
        queries: content.querySelector('#template-cell') === cell
          && content.getElementsByTagName('td').length === 1
          && template.querySelector('#template-cell') === null
          && response.getElementById('template-cell') === null
          && response.querySelector('#template-cell') === null,
        serialization: template.innerHTML.includes('id="template-cell"')
          && serializer.serializeToString(template).includes(
            '<template id="template"><table>',
          )
          && serializer.serializeToString(content).includes(
            '<td id="template-cell">template</td>',
          ),
        immutable: appendError === 'NoModificationAllowedError'
          && Object.isFrozen(content),
      };
      resolveTemplate(result);
    } catch (error) {
      rejectTemplate(error);
    }
  };
  request.onerror = () => rejectTemplate(new Error('template XHR failed'));
  request.send();
}));"###;
    let mut page_html = String::from("<script>");
    page_html.push_str(page_script);
    page_html.push_str("</script><script>");
    page_html.push_str(template_page_script);
    page_html.push_str("</script>");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/html-document",
            "/html-default",
            "/html-invalid",
            "/html-windows-1252",
            "/html-override-charset",
            "/html-utf16-bom",
            "/html-header-shift-jis",
            "/html-meta-shift-jis",
            "/html-meta-content-gbk",
            "/html-template",
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .unwrap_or_else(|_| {
                    panic!("timed out waiting for HTML XHR request {expected_path}")
                })
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(
                request.split_whitespace().nth(1),
                Some(expected_path),
                "unexpected request while awaiting {expected_path}: {request}"
            );
            let (content_type, body): (&str, Vec<u8>) = match expected_path {
                "/page" => ("text/html", page_html.as_bytes().to_vec()),
                "/html-document" => (
                    "text/html; charset=UTF-8",
                    r#"<!DOCTYPE HTML><HTML data-root='yes'><HEAD><TITLE>Native &amp; HTML</TITLE><META charset='utf-8'><SCRIPT id='raw'>if (a < b) rawValue();</SCRIPT><SCRIPT id='inert'>globalThis.__glassXhrProcessScriptRan = true;</SCRIPT></HEAD><BODY><P id='first' data-value='one &amp; two'>one<DIV id='second'>two<BR>three</DIV><INPUT disabled><IMG id='detached-image' src='/xhr-document-resource'><TEXTAREA id='rc'>A &amp; B</TEXTAREA><!--comment--></BODY></HTML>"#.as_bytes().to_vec(),
                ),
                "/html-windows-1252" => (
                    "text/html; charset=windows-1252",
                    b"<!doctype html><meta charset=shift_jis><title>caf\xe9</title><p>Price: \x80</p>".to_vec(),
                ),
                "/html-override-charset" => (
                    "text/html; charset=utf-8",
                    b"<!doctype html><title>caf\xe9</title><p>Price: \x80</p>".to_vec(),
                ),
                "/html-utf16-bom" => {
                    let mut body = vec![0xff, 0xfe];
                    body.extend(
                        "<!doctype html><title>café</title>"
                            .encode_utf16()
                            .flat_map(u16::to_le_bytes),
                    );
                    ("text/html; charset=windows-1252", body)
                },
                "/html-header-shift-jis" => (
                    "text/html; charset=Shift_JIS",
                    b"<!doctype html><title>\x93\xfa\x96\x7b\x8c\xea</title>".to_vec(),
                ),
                "/html-meta-shift-jis" => (
                    "text/html; charset=unknown-native-label",
                    b"<!doctype html><meta charset=Shift_JIS><title>\x93\xfa\x96\x7b\x8c\xea</title>".to_vec(),
                ),
                "/html-meta-content-gbk" => (
                    "text/html; charset=unknown-native-label",
                    b"<!doctype html><meta http-equiv=Content-Type content=\"text/html; charset=gbk\"><title>\xd6\xd0\xce\xc4</title>".to_vec(),
                ),
                "/html-template" => (
                    "text/html; charset=utf-8",
                    b"<!doctype html><template id='template'><table><tr><td id='template-cell'>template</td></tr></table><template id='nested-template'><b>nested</b></template></template>".to_vec(),
                ),
                "/html-default" => ("text/html", b"<p>default-html</p>".to_vec()),
                "/html-invalid" => ("text/html", b"<html><body><!--unterminated".to_vec()),
                other => panic!("unexpected XHR HTML request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        }
        match tokio::time::timeout(Duration::from_millis(250), listener.accept()).await {
            Ok(Ok((mut stream, _))) => {
                let request = read_http_request(&mut stream).await;
                panic!(
                    "XHR response document unexpectedly fetched a referenced resource: {request}"
                );
            }
            Ok(Err(error)) => panic!("XHR response-document listener failed: {error}"),
            Err(_) => {}
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("await htmlResultPromise")
            .await
            .unwrap(),
        serde_json::json!({
            "explicit": {
                "identity": true,
                "contentType": "text/html",
                "responseUrl": true,
                "responseTextError": "InvalidStateError",
                "root": ["HTML", "html", "HTML"],
                "title": "Native & HTML",
                "head": "HEAD",
                "body": "BODY",
                "bodyText": true,
                "scriptText": "if (a < b) rawValue();",
                "inertScript": true,
                "detachedImage": "/xhr-document-resource",
                "textareaText": "A & B",
                "firstAttribute": "one & two",
                "firstParent": true,
                "secondIdentity": true,
                "tagMatches": 1,
                "namespaceMatches": 1,
                "voids": true,
                "doctype": "html",
                "compatMode": "CSS1Compat",
                "serialized": true,
                "immutable": true,
            },
            "default": {
                "response": "<p>default-html</p>",
                "responseText": "<p>default-html</p>",
                "responseXml": null,
            },
            "charset": {
                "title": "café",
                "body": "Price: €",
            },
            "overrideCharset": {
                "title": "café",
                "body": "Price: €",
            },
            "utf16Bom": "café",
            "headerShiftJis": "日本語",
            "metaCharset": "日本語",
            "metaContent": "中文",
            "template": {
                "templateType": true,
                "contentIdentity": true,
                "ownerDocument": true,
                "parents": true,
                "queries": true,
                "serialization": true,
                "immutable": true,
            },
            "invalid": {
                "identity": true,
                "documentElement": "html",
                "recoveredComment": true,
                "compatMode": "BackCompat",
                "responseTextError": "InvalidStateError",
            },
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
