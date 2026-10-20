use super::*;

#[tokio::test]
async fn native_file_document_loads_rooted_media_during_navigation() {
    let root =
        std::env::temp_dir().join(format!("glass-native-file-static-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let media_path = root.join("clip.wav");
    let page_path = root.join("index.html");
    fs::write(&media_path, native_test_wav_bytes()).unwrap();
    fs::write(
        &page_path,
        "<html><body><audio id='audio' src='clip.wav'></audio></body></html>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let media_url = native_test_file_url(&media_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const audio = document.getElementById('audio'); return [audio.readyState, audio.networkState, audio.currentSrc, audio.duration, audio.error, audio.canPlayType('audio/wav')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([1, 1, media_url, 0.04, serde_json::Value::Null, "maybe"])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_loads_rooted_media_after_dynamic_attachment() {
    let root =
        std::env::temp_dir().join(format!("glass-native-file-dynamic-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let media_path = root.join("late.wav");
    let page_path = root.join("index.html");
    fs::write(&media_path, native_test_wav_bytes()).unwrap();
    fs::write(
        &page_path,
        "<html><body><div id='host'></div></body></html>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let media_url = native_test_file_url(&media_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let media_literal = serde_json::to_string(&media_url).unwrap();
    assert_eq!(
        engine
            .evaluate_async(format!(
                "(() => {{ globalThis.fileMediaLog = []; const audio = document.createElement('audio'); audio.addEventListener('load', () => fileMediaLog.push('load')); audio.addEventListener('error', () => fileMediaLog.push('error')); audio.src = {media_literal}; document.getElementById('host').appendChild(audio); globalThis.fileAudio = audio; return audio.src; }})()"
            ))
            .await
            .unwrap(),
        serde_json::json!(media_url)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[fileMediaLog, fileAudio.readyState, fileAudio.networkState, fileAudio.currentSrc, fileAudio.duration, fileAudio.error]",
            )
            .await
            .unwrap(),
        serde_json::json!([["load"], 1, 1, media_url, 0.04, serde_json::Value::Null])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_media_rejects_paths_outside_configured_root() {
    let root =
        std::env::temp_dir().join(format!("glass-native-file-allowed-{}", std::process::id()));
    let outside =
        std::env::temp_dir().join(format!("glass-native-file-outside-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside);
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let page_path = root.join("index.html");
    let media_path = outside.join("secret.wav");
    fs::write(&media_path, native_test_wav_bytes()).unwrap();
    fs::write(
        &page_path,
        "<html><body><div id='host'></div></body></html>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let media_url = native_test_file_url(&media_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let media_literal = serde_json::to_string(&media_url).unwrap();
    assert_eq!(
        engine
            .evaluate_async(format!(
                "(() => {{ globalThis.fileMediaLog = []; const audio = document.createElement('audio'); audio.addEventListener('error', () => fileMediaLog.push('error')); audio.src = {media_literal}; document.getElementById('host').appendChild(audio); globalThis.fileAudio = audio; return audio.src; }})()"
            ))
            .await
            .unwrap(),
        serde_json::json!(media_url)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[fileMediaLog, fileAudio.readyState, fileAudio.networkState, fileAudio.error && fileAudio.error.code]",
            )
            .await
            .unwrap(),
        serde_json::json!([["error"], 0, 3, 4])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[tokio::test]
async fn native_file_module_graph_keeps_fragment_distinct_identities() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-module-fragments-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("index.html"),
        "<script type='module' src='./entry.js#root'></script>",
    )
    .unwrap();
    fs::write(
        root.join("entry.js"),
        "import './dep.js#first'; import './dep.js#second'; globalThis.fileFragmentModuleReady = true;",
    )
    .unwrap();
    fs::write(
        root.join("dep.js"),
        "globalThis.fileFragmentModuleExecutions = (globalThis.fileFragmentModuleExecutions || 0) + 1; export const execution = globalThis.fileFragmentModuleExecutions;",
    )
    .unwrap();

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(native_test_file_url(&root.join("index.html")))
            .with_allowed_file_root(root.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.fileFragmentModuleExecutions, globalThis.fileFragmentModuleReady]",
            )
            .await
            .unwrap(),
        serde_json::json!([2, true])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_loads_rooted_script_stylesheet_and_image() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-subresources-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let page_path = root.join("index.html");
    let script_path = root.join("app.js");
    let module_path = root.join("module.js");
    let module_dependency_path = root.join("module-dependency.js");
    let module_dynamic_dependency_path = root.join("module-dynamic-dependency.js");
    let stylesheet_dir = root.join("styles");
    fs::create_dir_all(&stylesheet_dir).unwrap();
    let stylesheet_theme_dir = stylesheet_dir.join("theme");
    fs::create_dir_all(&stylesheet_theme_dir).unwrap();
    let stylesheet_path = stylesheet_dir.join("style.css");
    let imported_stylesheet_path = stylesheet_theme_dir.join("theme.css");
    let cycle_stylesheet_path = stylesheet_dir.join("cycle.css");
    let stylesheet_image_path = stylesheet_theme_dir.join("theme.png");
    let image_path = root.join("dot.png");
    fs::write(
        &script_path,
        "globalThis.fileScriptValue = getComputedStyle(document.getElementById('target')).color;",
    )
    .unwrap();
    fs::write(
        &module_path,
        "import { value } from './module-dependency.js'; globalThis.fileModuleValue = value; import('./module-' + 'dynamic-dependency.js').then(({ value }) => { globalThis.fileDynamicModuleValue = value; });",
    )
    .unwrap();
    fs::write(
        &module_dynamic_dependency_path,
        "export const value = 'module-dynamic-loaded';",
    )
    .unwrap();
    fs::write(
        &module_dependency_path,
        "export const value = 'module-loaded';",
    )
    .unwrap();
    fs::write(
        &stylesheet_path,
        "@import 'theme/theme\\2e css' layer(theme) supports(display: grid) screen and (min-width: 1000px); @import 'missing-print.css' print; @import 'missing-narrow.css' screen and (max-width: 600px); @import url('theme/theme\\2e css');",
    )
    .unwrap();
    fs::write(
        &imported_stylesheet_path,
        "@import '../cycle.css'; #target { width: 2px; height: 2px; color: rgb(1, 2, 3); background-image: url('theme\\2e png'); background-repeat: no-repeat; }",
    )
    .unwrap();
    fs::write(&cycle_stylesheet_path, "@import 'theme/theme.css';").unwrap();
    fs::write(&stylesheet_image_path, native_test_png_bytes()).unwrap();
    fs::write(&image_path, native_test_png_bytes()).unwrap();
    fs::write(
        &page_path,
        "<html><head><link rel='stylesheet' href='styles/style.css'></head><body><div id='target'>rooted</div><img id='image' src='dot.png'><script src='app.js'></script><script type='module' src='module.js'></script><script>globalThis.fileScriptEvents = []; document.querySelector('script[src]').addEventListener('load', () => fileScriptEvents.push('load'));</script></body></html>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let image_url = native_test_file_url(&image_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [fileScriptValue, fileModuleValue, fileDynamicModuleValue, fileScriptEvents, getComputedStyle(document.getElementById('target')).color, image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "rgb(1, 2, 3)",
            "module-loaded",
            "module-dynamic-loaded",
            ["load"],
            "rgb(1, 2, 3)",
            true,
            2,
            2,
            image_url
        ])
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
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_resolves_runtime_valued_imports_from_initial_scripts() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-runtime-imports-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let site = root.join("site");
    fs::create_dir_all(site.join("classic")).unwrap();
    fs::create_dir_all(site.join("module")).unwrap();
    fs::write(
        root.join("secret.js"),
        "export const value = 'must-not-leak';",
    )
    .unwrap();
    fs::write(
        site.join("index.html"),
        "<html><head><script src='classic-entry.js'></script><script type='module' src='module-entry.js'></script></head><body>rooted</body></html>",
    )
    .unwrap();
    fs::write(
        site.join("classic-entry.js"),
        r#"globalThis.fileClassicImportState = "pending";
           const target = "./classic/main.js";
           Promise.all([
             import(target),
             import(target),
             import("../secret.js").then(() => "unexpected", () => "rejected"),
           ]).then(async ([first, second, escape]) => {
             globalThis.fileClassicImportState = first.value;
             globalThis.fileClassicImportIdentity = first === second;
             globalThis.fileClassicNestedValue = await first.nested;
             globalThis.fileEscapeImportState = escape;
           }, error => { globalThis.fileClassicImportError = String(error); });"#,
    )
    .unwrap();
    fs::write(
        site.join("classic/main.js"),
        "export const value = 'classic-loaded'; const nestedTarget = './nested.js'; export const nested = import(nestedTarget).then(({ value }) => value);",
    )
    .unwrap();
    fs::write(
        site.join("classic/nested.js"),
        "export const value = 'classic-nested-loaded';",
    )
    .unwrap();
    fs::write(
        site.join("module-entry.js"),
        "globalThis.fileModuleImportState = 'pending'; const target = './module/main.js'; import(target).then(async module => { globalThis.fileModuleImportState = module.value; globalThis.fileModuleNestedValue = await module.nested; globalThis.fileModuleJsonValue = await module.jsonValue; }, error => { globalThis.fileModuleImportError = String(error); });",
    )
    .unwrap();
    fs::write(
        site.join("module/main.js"),
        "export const value = 'module-loaded'; const nestedTarget = './nested.js'; export const nested = import(nestedTarget).then(({ value }) => value); const jsonTarget = './data.json'; export const jsonValue = import(jsonTarget, { with: { type: 'json' } }).then(({ default: data }) => data.kind);",
    )
    .unwrap();
    fs::write(
        site.join("module/nested.js"),
        "export const value = 'module-nested-loaded';",
    )
    .unwrap();
    fs::write(
        site.join("module/data.json"),
        r#"{"kind":"module-json-loaded"}"#,
    )
    .unwrap();

    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&site.join("index.html")))
        .with_allowed_file_root(site.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.fileClassicImportState, globalThis.fileClassicImportIdentity ?? null, globalThis.fileClassicNestedValue ?? null, globalThis.fileEscapeImportState ?? null, globalThis.fileModuleImportState, globalThis.fileModuleNestedValue ?? null, globalThis.fileModuleJsonValue ?? null, globalThis.fileClassicImportError ?? null, globalThis.fileModuleImportError ?? null]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "classic-loaded",
            true,
            "classic-nested-loaded",
            "rejected",
            "module-loaded",
            "module-nested-loaded",
            "module-json-loaded",
            null,
            null
        ])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_rooted_file_csp_enforces_script_sources_and_replaces_on_reload() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-script-csp-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let site = root.join("site");
    let other_root = root.join("other");
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(&other_root).unwrap();
    fs::write(
        other_root.join("other.js"),
        "globalThis.fileCspOtherRoot = 'must-not-run';",
    )
    .unwrap();
    fs::write(
        site.join("classic.js"),
        "globalThis.fileCspClassic = 'same-root-classic';",
    )
    .unwrap();
    fs::write(
        site.join("dependency.js"),
        "export const value = 'same-root-module';",
    )
    .unwrap();
    fs::write(
        site.join("entry.js"),
        "import { value } from './dependency.js'; globalThis.fileCspModule = value;",
    )
    .unwrap();
    fs::write(
        site.join("dynamic.js"),
        "export const value = 'same-root-dynamic';",
    )
    .unwrap();
    let page_url = native_test_file_url(&site.join("index.html"));
    let other_script_url = native_test_file_url(&other_root.join("other.js"));
    fs::write(
        site.join("index.html"),
        format!(
            r#"<!doctype html><html><head>
              <meta http-equiv="Content-Security-Policy" content="script-src 'self' 'nonce-bootstrap'">
              <script src="classic.js"></script>
              <script type="module" src="entry.js"></script>
              <script nonce="bootstrap">
                globalThis.fileCspInline = 'nonce-allowed';
                globalThis.fileCspDynamicState = 'pending';
                import('./dynamic.js').then(
                  module => globalThis.fileCspDynamicState = module.value,
                  () => globalThis.fileCspDynamicState = 'rejected'
                );
                globalThis.fileCspCrossRootState = 'pending';
                import({other_script_url:?}).then(
                  () => globalThis.fileCspCrossRootState = 'unexpected',
                  () => globalThis.fileCspCrossRootState = 'rejected'
                );
              </script>
            </head><body>rooted CSP</body></html>"#
        ),
    )
    .unwrap();

    let config = NativeEngineConfig::default()
        .with_initial_url(page_url.clone())
        .with_allowed_file_root(site.clone())
        .with_allowed_file_root(other_root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "[fileCspClassic ?? null, fileCspModule ?? null, fileCspInline ?? null, fileCspDynamicState ?? null, fileCspCrossRootState ?? null, globalThis.fileCspOtherRoot ?? null]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "same-root-classic",
            "same-root-module",
            "nonce-allowed",
            "same-root-dynamic",
            "rejected",
            null
        ])
    );

    fs::write(
        site.join("index.html"),
        "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'none'\"><script src=\"classic.js\"></script><script>globalThis.fileCspBlockedInline = true;</script></head><body>csp reload</body></html>",
    )
    .unwrap();
    fs::write(
        site.join("classic.js"),
        "globalThis.fileCspClassic = 'must-not-run-after-reload';",
    )
    .unwrap();
    engine.navigate_async(page_url).await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[document.body.textContent, globalThis.fileCspClassic, 'fileCspBlockedInline' in globalThis]",
            )
            .await
            .unwrap(),
        serde_json::json!(["csp reload", null, false])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_applies_parser_import_maps_to_initial_scripts() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-parser-import-map-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let site = root.join("site");
    fs::create_dir_all(site.join("mapped")).unwrap();
    fs::write(
        root.join("secret.js"),
        "export const value = 'must-not-leak';",
    )
    .unwrap();
    fs::write(
        site.join("index.html"),
        r#"<html><head>
          <script type="importmap">{"imports":{"local-package":"./mapped/package.js","escape-package":"../secret.js"}}</script>
          <script type="module" src="entry.js"></script>
          <script type="importmap">{"imports":{"local-package":"./mapped/alternate.js","later-package":"./mapped/later.js"}}</script>
          <script type="importmap">{malformed</script>
          <script>
            globalThis.fileParserImportMapClassic = "pending";
            const localSpecifier = "local-package";
            const laterSpecifier = "later-package";
            const escapeSpecifier = "escape-package";
            Promise.all([
              import(localSpecifier).then(({ value }) => value),
              import(laterSpecifier).then(({ value }) => value),
              import(escapeSpecifier).then(() => "unexpected", () => "rejected"),
            ]).then(([local, later, escaped]) => {
              globalThis.fileParserImportMapClassic = [local, later, escaped];
            }, error => {
              globalThis.fileParserImportMapClassicError = String(error);
            });
          </script>
        </head><body>rooted import maps</body></html>"#,
    )
    .unwrap();
    fs::write(
        site.join("entry.js"),
        "import { value } from 'local-package'; globalThis.fileParserImportMapStatic = value; const specifier = 'later-package'; import(specifier).then(({ value }) => { globalThis.fileParserImportMapModuleRuntime = value; }, error => { globalThis.fileParserImportMapModuleError = String(error); });",
    )
    .unwrap();
    fs::write(
        site.join("mapped/package.js"),
        "import { value as nested } from './nested.js'; export const value = `primary:${nested}`;",
    )
    .unwrap();
    fs::write(
        site.join("mapped/nested.js"),
        "export const value = 'nested';",
    )
    .unwrap();
    fs::write(
        site.join("mapped/alternate.js"),
        "export const value = 'alternate';",
    )
    .unwrap();
    fs::write(
        site.join("mapped/later.js"),
        "export const value = 'later';",
    )
    .unwrap();

    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&site.join("index.html")))
        .with_allowed_file_root(site.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.fileParserImportMapStatic ?? null, globalThis.fileParserImportMapModuleRuntime ?? null, globalThis.fileParserImportMapClassic ?? null, globalThis.fileParserImportMapModuleError ?? null, globalThis.fileParserImportMapClassicError ?? null]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "primary:nested",
            "later",
            ["primary:nested", "later", "rejected"],
            null,
            null
        ])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_resolves_runtime_imports_from_late_classic_scripts() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-late-runtime-imports-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let site = root.join("site");
    fs::create_dir_all(site.join("late")).unwrap();
    fs::write(
        site.join("index.html"),
        "<html><head></head><body>rooted</body></html>",
    )
    .unwrap();
    fs::write(
        site.join("late-entry.js"),
        "globalThis.lateFileImportState = 'pending'; const target = 'late-package'; import(target).then(async module => { globalThis.lateFileImportState = module.value; globalThis.lateFileNestedValue = await module.loadNested(); }, error => { globalThis.lateFileImportError = String(error); });",
    )
    .unwrap();
    fs::write(
        site.join("late/main.js"),
        "export const value = 'late-module-loaded'; export async function loadNested() { const target = './nested.js'; return (await import(target)).value; }",
    )
    .unwrap();
    fs::write(
        site.join("late/nested.js"),
        "export const value = 'late-nested-loaded';",
    )
    .unwrap();
    fs::write(
        site.join("late/module-entry.js"),
        "globalThis.lateFileModuleImportState = 'pending'; const target = 'late-module-package'; import(target).then(async module => { globalThis.lateFileModuleImportState = module.value; globalThis.lateFileModuleNestedValue = await module.nested; }, error => { globalThis.lateFileModuleImportError = String(error); });",
    )
    .unwrap();
    fs::write(
        site.join("late/module.js"),
        "export const value = 'late-module-script-loaded'; const nestedTarget = './module-nested.js'; export const nested = import(nestedTarget).then(({ value }) => value);",
    )
    .unwrap();
    fs::write(
        site.join("late/module-nested.js"),
        "export const value = 'late-module-nested-loaded';",
    )
    .unwrap();
    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&site.join("index.html")))
        .with_allowed_file_root(site.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const map = document.createElement('script'); map.type = 'importmap'; map.textContent = JSON.stringify({ imports: { 'late-package': './late/main.js', 'late-module-package': './late/module.js' } }); document.head.append(map); const script = document.createElement('script'); script.src = 'late-entry.js'; document.head.append(script); const module = document.createElement('script'); module.type = 'module'; module.src = 'late/module-entry.js'; document.head.append(module); return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.lateFileImportState, globalThis.lateFileNestedValue ?? null, globalThis.lateFileModuleImportState, globalThis.lateFileModuleNestedValue ?? null, globalThis.lateFileImportError ?? null, globalThis.lateFileModuleImportError ?? null]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "late-module-loaded",
            "late-nested-loaded",
            "late-module-script-loaded",
            "late-module-nested-loaded",
            null,
            null
        ])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_loads_rooted_dynamic_subresources() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-dynamic-resources-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let page_path = root.join("index.html");
    let script_path = root.join("late.js");
    let module_path = root.join("late-module.js");
    let module_dependency_path = root.join("late-module-dependency.js");
    let stylesheet_dir = root.join("styles");
    fs::create_dir_all(&stylesheet_dir).unwrap();
    let stylesheet_path = stylesheet_dir.join("late.css");
    let imported_stylesheet_path = stylesheet_dir.join("theme.css");
    let image_path = stylesheet_dir.join("late.png");
    fs::write(
        &script_path,
        "globalThis.dynamicFileScriptValue = 'executed';",
    )
    .unwrap();
    fs::write(
        &module_path,
        "import { value } from './late-module-dependency.js'; globalThis.dynamicFileModuleValue = value;",
    )
    .unwrap();
    fs::write(
        &module_dependency_path,
        "export const value = 'module-executed';",
    )
    .unwrap();
    fs::write(&stylesheet_path, "@import 'theme.css';").unwrap();
    fs::write(
        &imported_stylesheet_path,
        "#target { width: 2px; height: 2px; color: rgb(4, 5, 6); background-image: url('late.png'); background-repeat: no-repeat; }",
    )
    .unwrap();
    fs::write(&image_path, native_test_png_bytes()).unwrap();
    fs::write(
        &page_path,
        "<html><body><div id='host'></div><div id='target'>rooted</div><script>globalThis.dynamicFileEvents = [];</script></body></html>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let script_url = native_test_file_url(&script_path);
    let module_url = native_test_file_url(&module_path);
    let stylesheet_url = native_test_file_url(&stylesheet_path);
    let image_url = native_test_file_url(&image_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let script_literal = serde_json::to_string(&script_url).unwrap();
    let module_literal = serde_json::to_string(&module_url).unwrap();
    let stylesheet_literal = serde_json::to_string(&stylesheet_url).unwrap();
    let image_literal = serde_json::to_string(&image_url).unwrap();
    engine
        .evaluate_async(format!(
            r#"(() => {{
                const host = document.getElementById('host');
                const script = document.createElement('script');
                script.src = {script_literal};
                script.addEventListener('load', () => dynamicFileEvents.push('script-load'));
                script.addEventListener('error', () => dynamicFileEvents.push('script-error'));
                const map = document.createElement('script');
                map.type = 'importmap';
                map.textContent = '{{"imports":{{"mapped":"./late-module-dependency.js"}}}}';
                const inlineModule = document.createElement('script');
                inlineModule.type = 'module';
                inlineModule.textContent = "import {{ value }} from 'mapped'; globalThis.dynamicMappedFileModuleValue = value;";
                const module = document.createElement('script');
                module.type = 'module';
                module.src = {module_literal};
                module.addEventListener('load', () => dynamicFileEvents.push('module-load'));
                module.addEventListener('error', () => dynamicFileEvents.push('module-error'));
                const link = document.createElement('link');
                link.rel = 'stylesheet';
                link.href = {stylesheet_literal};
                link.addEventListener('load', () => dynamicFileEvents.push('style-load'));
                link.addEventListener('error', () => dynamicFileEvents.push('style-error'));
                const image = document.createElement('img');
                image.src = {image_literal};
                image.addEventListener('load', () => dynamicFileEvents.push('image-load'));
                image.addEventListener('error', () => dynamicFileEvents.push('image-error'));
                host.append(script, map, inlineModule, module, link, image);
                globalThis.dynamicFileImage = image;
                return true;
            }})()"#,
        ))
        .await
        .unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "[dynamicFileScriptValue, dynamicFileModuleValue, dynamicMappedFileModuleValue, dynamicFileEvents, getComputedStyle(document.getElementById('target')).color, dynamicFileImage.complete, dynamicFileImage.naturalWidth, dynamicFileImage.currentSrc]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "executed",
            "module-executed",
            "module-executed",
            ["script-load", "module-load", "style-load", "image-load"],
            "rgb(4, 5, 6)",
            true,
            2,
            image_url
        ])
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
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_stylesheet_import_rejects_unrooted_dependency() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-stylesheet-import-root-{}",
        std::process::id()
    ));
    let outside_path = std::env::temp_dir().join(format!(
        "glass-native-file-stylesheet-import-outside-{}.css",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_file(&outside_path);
    fs::create_dir_all(&root).unwrap();
    let page_path = root.join("index.html");
    let stylesheet_path = root.join("style.css");
    fs::write(&outside_path, "#target { color: rgb(9, 8, 7); }").unwrap();
    let outside_url = native_test_file_url(&outside_path);
    fs::write(
        &stylesheet_path,
        format!("@import {outside_url:?}; #target {{ color: rgb(9, 8, 7); }}"),
    )
    .unwrap();
    fs::write(
        &page_path,
        "<html><head><link id='style' rel='stylesheet' href='style.css'></head><body><div id='target'>rooted</div></body></html>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("getComputedStyle(document.getElementById('target')).color")
            .await
            .unwrap(),
        serde_json::json!("rgb(0, 0, 0)")
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_file(outside_path).unwrap();
}

#[tokio::test]
async fn native_file_inline_style_csp_enforces_nonce_hash_and_runtime_mutations() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-inline-style-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    let page_path = site.join("index.html");
    let nonce_style = "#nonce-target { color: rgb(1, 2, 3); }";
    let hash_style = "#hash-target { color: rgb(4, 5, 6); }";
    let allowed_attribute = "color: rgb(7, 8, 9);";
    let blocked_attribute = "color: rgb(10, 11, 12);";
    let style_element_hash =
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(hash_style.as_bytes()));
    let style_attribute_hash = base64::engine::general_purpose::STANDARD
        .encode(Sha256::digest(allowed_attribute.as_bytes()));
    fs::write(
        &page_path,
        format!(
            "<html><head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval'; style-src 'self'; style-src-elem 'nonce-Style123' 'sha256-{style_element_hash}'; style-src-attr 'unsafe-hashes' 'sha256-{style_attribute_hash}'\"><style nonce=\"Style123\">{nonce_style}</style><style>#blocked-target {{ color: rgb(9, 8, 7); }}</style><style>{hash_style}</style></head><body><div id=\"nonce-target\">nonce</div><div id=\"blocked-target\">blocked</div><div id=\"hash-target\">hash</div><div id=\"attribute-hash-target\" style=\"{allowed_attribute}\">attribute hash</div><div id=\"attribute-blocked-target\" style=\"{blocked_attribute}\">attribute blocked</div><div id=\"dynamic-style-target\">dynamic style</div><div id=\"initial-script-target\">initial script</div><div id=\"dynamic-script-target\">dynamic script</div><div id=\"dynamic-attribute-target\">dynamic attribute</div><script>const style = document.createElement('style'); style.setAttribute('nonce', 'Style123'); style.textContent = '#initial-script-target {{ color: rgb(16, 17, 18); }}'; document.head.appendChild(style);</script></body></html>"
        ),
    )
    .unwrap();

    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(site);
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[getComputedStyle(document.getElementById('nonce-target')).color, getComputedStyle(document.getElementById('blocked-target')).color, getComputedStyle(document.getElementById('hash-target')).color, getComputedStyle(document.getElementById('attribute-hash-target')).color, getComputedStyle(document.getElementById('attribute-blocked-target')).color, getComputedStyle(document.getElementById('initial-script-target')).color]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "rgb(1, 2, 3)",
            "rgb(0, 0, 0)",
            "rgb(4, 5, 6)",
            "rgb(7, 8, 9)",
            "rgb(0, 0, 0)",
            "rgb(16, 17, 18)"
        ])
    );

    engine
        .evaluate_async(&format!(
            r#"(() => {{
                const allowedStyle = document.createElement('style');
                allowedStyle.setAttribute('nonce', 'Style123');
                allowedStyle.textContent = '#dynamic-style-target {{ color: rgb(13, 14, 15); }}';
                document.head.appendChild(allowedStyle);
                const blockedStyle = document.createElement('style');
                blockedStyle.textContent = '#dynamic-style-target {{ color: rgb(16, 17, 18); }}';
                document.head.appendChild(blockedStyle);
                const dynamicScript = document.createElement('script');
                dynamicScript.textContent = "const style = document.createElement('style'); style.setAttribute('nonce', 'Style123'); style.textContent = '#dynamic-script-target {{ color: rgb(19, 20, 21); }}'; document.head.appendChild(style);";
                document.head.appendChild(dynamicScript);
                document.getElementById('attribute-hash-target').setAttribute('style', {blocked_attribute:?});
                document.getElementById('dynamic-attribute-target').setAttribute('style', {allowed_attribute:?});
                return true;
            }})()"#
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[getComputedStyle(document.getElementById('dynamic-style-target')).color, getComputedStyle(document.getElementById('dynamic-script-target')).color, getComputedStyle(document.getElementById('attribute-hash-target')).color, getComputedStyle(document.getElementById('dynamic-attribute-target')).color]",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "rgb(13, 14, 15)",
            "rgb(19, 20, 21)",
            "rgb(0, 0, 0)",
            "rgb(7, 8, 9)"
        ])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_dynamic_csp_meta_is_head_scoped_and_append_only() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-dynamic-csp-meta-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    let page_path = site.join("index.html");
    fs::write(
        &page_path,
        "<html><head><meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'unsafe-inline'\"></head><body></body></html>",
    )
    .unwrap();
    fs::write(
        site.join("late.js"),
        "globalThis.dynamicMetaState.externalRan = true;",
    )
    .unwrap();

    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&page_path))
        .with_allowed_file_root(&site);
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            r#"(() => {
                globalThis.dynamicMetaState = { bodyMetaRan: false, headMetaRan: false, errors: [] };
                const bodyMeta = document.createElement('meta');
                bodyMeta.setAttribute('http-equiv', 'Content-Security-Policy');
                bodyMeta.setAttribute('content', "script-src 'none'");
                document.body.appendChild(bodyMeta);
                const bodyScript = document.createElement('script');
                bodyScript.textContent = 'globalThis.dynamicMetaState.bodyMetaRan = true;';
                document.body.appendChild(bodyScript);
                return true;
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.dynamicMetaState.bodyMetaRan")
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    engine
        .evaluate_async(
            r#"(() => {
                const meta = document.createElement('meta');
                meta.setAttribute('id', 'dynamic-csp-policy');
                meta.setAttribute('http-equiv', 'Content-Security-Policy');
                meta.setAttribute('content', "script-src 'none'");
                document.head.appendChild(meta);
                globalThis.dynamicMetaState.capturedContent = meta.getAttribute('content');
                meta.setAttribute('content', "script-src 'unsafe-inline'");
                meta.remove();
                const script = document.createElement('script');
                script.addEventListener('error', () => globalThis.dynamicMetaState.errors.push('head'));
                script.textContent = 'globalThis.dynamicMetaState.headMetaRan = true;';
                document.body.appendChild(script);
                const external = document.createElement('script');
                external.addEventListener('error', () => globalThis.dynamicMetaState.errors.push('external'));
                external.src = 'late.js';
                document.body.appendChild(external);
                return true;
            })()"#,
        )
        .await
        .unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "({ capturedContent: globalThis.dynamicMetaState.capturedContent, headMetaRan: Boolean(globalThis.dynamicMetaState.headMetaRan), externalRan: Boolean(globalThis.dynamicMetaState.externalRan), errors: globalThis.dynamicMetaState.errors })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "capturedContent": "script-src 'none'",
            "headMetaRan": false,
            "externalRan": false,
            "errors": ["head", "external"]
        })
    );

    engine
        .evaluate_async(
            r#"(() => {
                const script = document.createElement('script');
                script.addEventListener('error', () => globalThis.dynamicMetaState.errors.push('retained'));
                script.textContent = 'globalThis.dynamicMetaState.headMetaRan = true;';
                document.body.appendChild(script);
                return true;
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ bodyMetaRan: globalThis.dynamicMetaState.bodyMetaRan, headMetaRan: Boolean(globalThis.dynamicMetaState.headMetaRan), errors: globalThis.dynamicMetaState.errors })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "bodyMetaRan": true,
            "headMetaRan": false,
            "errors": ["head", "external", "retained"]
        })
    );

    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_dynamic_csp_meta_blocks_same_turn_image_resource() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-dynamic-image-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    let page_path = site.join("index.html");
    let image_path = site.join("pixel.png");
    fs::write(
        &page_path,
        "<!doctype html><html><head></head><body></body></html>",
    )
    .unwrap();
    fs::write(&image_path, native_test_png_bytes()).unwrap();

    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&page_path))
        .with_allowed_file_root(&site);
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(&format!(
            r#"(() => {{
                globalThis.dynamicFileImageErrors = [];
                const policy = document.createElement('meta');
                policy.setAttribute('http-equiv', 'Content-Security-Policy');
                policy.setAttribute('content', "img-src 'none'");
                document.head.appendChild(policy);
                const image = document.createElement('img');
                image.setAttribute('width', '2');
                image.setAttribute('height', '2');
                image.addEventListener('error', () => dynamicFileImageErrors.push('blocked'));
                image.src = {image_url:?};
                document.body.appendChild(image);
                return true;
            }})()"#,
            image_url = native_test_file_url(&image_path),
        ))
        .await
        .unwrap();

    assert_eq!(
        engine
            .evaluate_async("dynamicFileImageErrors")
            .await
            .unwrap(),
        serde_json::json!(["blocked"])
    );
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. }))
    );

    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_stylesheet_csp_enforces_self_nonce_and_dynamic_error() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-style-csp-root-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let other = root.join("other");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(&other).unwrap();

    let page_path = site.join("index.html");
    let stylesheet_path = site.join("main.css");
    let dependency_path = site.join("dependency.css");
    let nonce_stylesheet_path = other.join("nonce.css");
    let denied_stylesheet_path = other.join("denied.css");
    let dynamic_stylesheet_path = other.join("dynamic-denied.css");
    fs::write(
        &stylesheet_path,
        "@import './dependency.css'; #target { color: rgb(1, 2, 3); }",
    )
    .unwrap();
    fs::write(
        &dependency_path,
        "#target { background-color: rgb(4, 5, 6); }",
    )
    .unwrap();
    fs::write(&nonce_stylesheet_path, "#target { color: rgb(7, 8, 9); }").unwrap();
    fs::write(&denied_stylesheet_path, "#target { color: rgb(9, 8, 7); }").unwrap();
    fs::write(&dynamic_stylesheet_path, "#target { color: rgb(6, 5, 4); }").unwrap();
    let nonce_stylesheet_url = native_test_file_url(&nonce_stylesheet_path);
    let denied_stylesheet_url = native_test_file_url(&denied_stylesheet_path);
    fs::write(
        &page_path,
        format!(
            "<html><head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'self' 'nonce-Style123'; script-src 'unsafe-inline' 'unsafe-eval'\"><link rel=\"stylesheet\" href=\"main.css\"><link rel=\"stylesheet\" nonce=\"Style123\" href=\"{nonce_stylesheet_url}\"><link id=\"blocked\" rel=\"stylesheet\" href=\"{denied_stylesheet_url}\"></head><body><div id=\"target\">rooted</div></body></html>"
        ),
    )
    .unwrap();

    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(site)
        .with_allowed_file_root(other);
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .effects_since(0)
            .unwrap()
            .effects
            .iter()
            .filter(|effect| effect.kind == NativeEventKind::Error)
            .count(),
        1
    );

    assert_eq!(
        engine
            .evaluate_async(
                "[getComputedStyle(document.getElementById('target')).color, getComputedStyle(document.getElementById('target')).backgroundColor]",
            )
            .await
            .unwrap(),
        serde_json::json!(["rgb(7, 8, 9)", "rgb(4, 5, 6)"])
    );

    let dynamic_stylesheet_url = native_test_file_url(&dynamic_stylesheet_path);
    engine
        .evaluate_async(&format!(
            r#"(() => {{
                globalThis.dynamicStyleCspEvents = [];
                const updated = document.getElementById('blocked');
                updated.addEventListener('error', () => dynamicStyleCspEvents.push('updated-error'));
                updated.href = {dynamic_stylesheet_url:?};
                const link = document.createElement('link');
                link.rel = 'stylesheet';
                link.href = {dynamic_stylesheet_url:?};
                link.addEventListener('load', () => dynamicStyleCspEvents.push('load'));
                link.addEventListener('error', () => dynamicStyleCspEvents.push('error'));
                document.head.appendChild(link);
                return true;
            }})()"#
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("dynamicStyleCspEvents")
            .await
            .unwrap(),
        serde_json::json!(["updated-error", "error"])
    );
    assert_eq!(
        engine
            .evaluate_async("getComputedStyle(document.getElementById('target')).color")
            .await
            .unwrap(),
        serde_json::json!("rgb(7, 8, 9)")
    );

    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_stylesheet_csp_blocks_import_from_another_allowed_root() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-style-import-csp-{}",
        std::process::id()
    ));
    let site = root.join("site");
    let other = root.join("other");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(&other).unwrap();
    let page_path = site.join("index.html");
    let stylesheet_path = site.join("main.css");
    let imported_path = other.join("imported.css");
    fs::write(&imported_path, "#target { color: rgb(9, 8, 7); }").unwrap();
    let imported_url = native_test_file_url(&imported_path);
    fs::write(
        &stylesheet_path,
        format!("@import {imported_url:?}; #target {{ color: rgb(1, 2, 3); }}"),
    )
    .unwrap();
    fs::write(
        &page_path,
        "<html><head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'self'\"><link rel=\"stylesheet\" href=\"main.css\"></head><body><div id=\"target\">rooted</div></body></html>",
    )
    .unwrap();

    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(site)
        .with_allowed_file_root(other);
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("getComputedStyle(document.getElementById('target')).color")
            .await
            .unwrap(),
        serde_json::json!("rgb(0, 0, 0)")
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_document_runs_dedicated_and_module_workers() {
    let root =
        std::env::temp_dir().join(format!("glass-native-file-workers-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let page_path = root.join("index.html");
    let worker_path = root.join("worker.js");
    let worker_dependency_path = root.join("worker-dependency.js");
    let module_worker_path = root.join("module-worker.js");
    let module_dependency_path = root.join("module-worker-dependency.js");
    fs::write(&worker_path, "importScripts('./worker-dependency.js'); postMessage({ kind: 'ready', href: self.location.href }); self.onmessage = event => postMessage({ kind: 'reply', value: fileWorkerDependency + event.data });").unwrap();
    fs::write(
        &worker_dependency_path,
        "globalThis.fileWorkerDependency = 3;",
    )
    .unwrap();
    fs::write(&module_worker_path, "import { value } from './module-worker-dependency.js#first'; import { value as second } from './module-worker-dependency.js#second'; globalThis.fileModuleWorkerDependencyValues = [value, second]; postMessage({ kind: 'module-ready', value, executions: globalThis.fileModuleDependencyExecutions }); self.onmessage = event => postMessage({ kind: 'module-reply', value: value + event.data });").unwrap();
    fs::write(&module_dependency_path, "globalThis.fileModuleDependencyExecutions = (globalThis.fileModuleDependencyExecutions || 0) + 1; export const value = 7;").unwrap();
    fs::write(&page_path, "<html><body><script>globalThis.fileWorkerMessages = []; globalThis.fileModuleMessages = []; globalThis.fileWorker = new Worker('worker.js'); fileWorker.onmessage = event => fileWorkerMessages.push(event.data); globalThis.fileModuleWorker = new Worker('module-worker.js', { type: 'module' }); fileModuleWorker.onmessage = event => fileModuleMessages.push(event.data);</script></body></html>").unwrap();
    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[fileWorkerMessages, fileModuleMessages, fileWorker.url, fileModuleWorker.url]"
            )
            .await
            .unwrap(),
        serde_json::json!([
            [{ "kind": "ready", "href": native_test_file_url(&worker_path) }],
            [{ "kind": "module-ready", "value": 7, "executions": 2 }],
            native_test_file_url(&worker_path), native_test_file_url(&module_worker_path),
        ])
    );
    engine
        .evaluate_async("fileWorker.postMessage(4); fileModuleWorker.postMessage(6); true")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("[fileWorkerMessages, fileModuleMessages]")
            .await
            .unwrap(),
        serde_json::json!([
            [
                { "kind": "ready", "href": native_test_file_url(&worker_path) },
                { "kind": "reply", "value": 7 },
            ],
            [
                { "kind": "module-ready", "value": 7, "executions": 2 },
                { "kind": "module-reply", "value": 13 },
            ],
        ])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_file_module_graph_rejects_unrooted_dependencies() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-module-root-{}",
        std::process::id()
    ));
    let outside_path = std::env::temp_dir().join(format!(
        "glass-native-file-module-outside-{}.js",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_file(&outside_path);
    fs::create_dir_all(&root).unwrap();
    let page_path = root.join("index.html");
    let module_path = root.join("module.js");
    fs::write(&outside_path, "globalThis.unrootedModuleValue = 'read';").unwrap();
    let outside_url = native_test_file_url(&outside_path);
    fs::write(
        &module_path,
        format!("import {outside_url:?}; globalThis.unrootedModuleValue = 'read';"),
    )
    .unwrap();
    fs::write(
        &page_path,
        r#"<html><body><script type='module' src='module.js'></script><script>globalThis.moduleEvents = []; document.querySelector("script[type='module']").addEventListener('error', () => moduleEvents.push('error')); </script></body></html>"#,
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("[moduleEvents, typeof unrootedModuleValue]")
            .await
            .unwrap(),
        serde_json::json!([["error"], "undefined"])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_file(outside_path).unwrap();
}

#[tokio::test]
async fn native_file_download_link_transfers_rooted_bytes() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-download-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let download_directory = std::env::temp_dir().join(format!(
        "glass-native-file-download-destination-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&download_directory).unwrap();
    let page_path = root.join("index.html");
    let asset_path = root.join("asset.bin");
    fs::write(
        &page_path,
        "<html><body><a id='file' href='asset.bin' download='report.bin'>Download</a></body></html>",
    )
    .unwrap();
    fs::write(&asset_path, b"rooted-file-download").unwrap();
    let page_url = native_test_file_url(&page_path);
    let config = NativeEngineConfig::default()
        .with_initial_url(page_url)
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=file".into(),
        })
        .await
        .unwrap();
    assert_eq!(engine.download_ids().unwrap(), vec!["native-download-1"]);
    let outcome = engine
        .wait_for_download_async(&download_directory, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(outcome.guid, "native-download-1");
    assert_eq!(outcome.suggested_filename, "report.bin");
    assert_eq!(outcome.state, "completed");
    assert_eq!(
        fs::read(download_directory.join("report.bin")).unwrap(),
        b"rooted-file-download"
    );
    assert_eq!(outcome.received_bytes, b"rooted-file-download".len() as u64);
    assert!(outcome.sha256.is_some());
    engine.close_async().await.unwrap();
    fs::remove_dir_all(&root).unwrap();
    fs::remove_dir_all(&download_directory).unwrap();
}

#[tokio::test]
async fn native_rooted_file_frame_csp_sees_runtime_meta_before_child_load() {
    let _guard = native_content_process_test_lock().lock().await;
    let root = std::env::temp_dir().join(format!(
        "glass-native-rooted-file-frame-policy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let site = root.join("site");
    fs::create_dir_all(&site).unwrap();
    let page_path = site.join("index.html");
    fs::write(site.join("child.html"), "<title>must not load</title>").unwrap();
    fs::write(
        &page_path,
        r#"<!doctype html><head>
          <meta http-equiv="Content-Security-Policy"
                content="script-src 'unsafe-inline'; frame-src 'self'">
        </head><body><script>
          const policy = document.createElement('meta');
          policy.setAttribute('http-equiv', 'Content-Security-Policy');
          policy.setAttribute('content', "frame-src 'none'");
          document.head.appendChild(policy);
          const frame = document.createElement('iframe');
          frame.src = 'child.html';
          document.body.appendChild(frame);
        </script></body>"#,
    )
    .unwrap();

    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&page_path))
        .with_allowed_file_root(&site);
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child = frames
        .iter()
        .find(|frame| frame.id.ends_with(":frame-1"))
        .expect("the denied iframe should remain represented");
    assert_eq!(child.url, "about:blank");
    session.close().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}
