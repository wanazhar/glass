use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NativeScriptCommand {
    Focus {
        node_index: u32,
    },
    Blur {
        node_index: u32,
    },
    Click {
        node_index: u32,
    },
    SubmitForm {
        node_index: u32,
    },
    RequestSubmitForm {
        node_index: u32,
        #[serde(default)]
        submitter_index: Option<u32>,
    },
    ResetForm {
        node_index: u32,
        perform_reset: bool,
    },
    Navigate {
        href: String,
        #[serde(default)]
        replace: bool,
    },
    HistoryPushState {
        href: String,
        state: serde_json::Value,
    },
    HistoryReplaceState {
        href: String,
        state: serde_json::Value,
    },
    HistoryGo {
        delta: i32,
    },
    ScrollTo {
        node_index: u32,
        left: i64,
        top: i64,
    },
    OpenWindow {
        href: String,
        target: String,
        #[serde(default)]
        handle: Option<String>,
    },
    SetWindowName {
        value: String,
    },
    CloseWindow {
        target: String,
        #[serde(default)]
        target_context_id: Option<String>,
    },
    NavigateWindow {
        target: String,
        #[serde(default)]
        target_context_id: Option<String>,
        href: String,
        #[serde(default)]
        replace: bool,
    },
    PostMessage {
        target: String,
        target_origin: String,
        data: serde_json::Value,
        #[serde(default)]
        target_context_id: Option<String>,
        #[serde(default)]
        transfer_ports: Vec<NativeMessagePortTransfer>,
    },
    Fetch {
        request_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
        href: String,
        credentials: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials_mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referrer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referrer_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referrer_policy: Option<String>,
        method: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        body_base64: Option<String>,
        #[serde(default)]
        content_type: Option<String>,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        redirect: Option<String>,
        #[serde(default)]
        cache: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upload_stream_id: Option<u32>,
        /// An internal request destination. This is never user-configurable
        /// through the public Fetch API; the page FontFace loader uses
        /// `font`, while module imports use `module` so the content owner can
        /// apply module-graph policy and preserve the active referrer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        destination: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        module_referrer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        module_type: Option<NativeModuleResourceType>,
    },
    /// Install bytes loaded by a page-realm `FontFace`. The fetch or inline
    /// source resolution happens in JavaScript first; this bounded command
    /// transfers only the admitted font bytes to the native document owner.
    FontFaceInstall {
        request_id: u32,
        family: String,
        weight: String,
        style: String,
        #[serde(default)]
        stretch: String,
        #[serde(default)]
        unicode_range: String,
        #[serde(default)]
        variant: String,
        #[serde(default)]
        feature_settings: String,
        #[serde(default)]
        size_adjust: String,
        #[serde(default)]
        ascent_override: String,
        #[serde(default)]
        descent_override: String,
        #[serde(default)]
        line_gap_override: String,
        #[serde(default)]
        display: String,
        #[serde(default)]
        variation_settings: String,
        body_base64: String,
    },
    ServiceWorkerRegister {
        request_id: u32,
        script_url: String,
        scope: String,
        #[serde(default)]
        worker_type: String,
        #[serde(default)]
        referrer_url: String,
        #[serde(default)]
        referrer_policy: String,
    },
    ServiceWorkerUnregister {
        request_id: u32,
        scope: String,
    },
    ServiceWorkerUpdate {
        request_id: u32,
        scope: String,
        #[serde(default)]
        referrer_url: String,
        #[serde(default)]
        referrer_policy: String,
    },
    ServiceWorkerNavigationPreload {
        request_id: u32,
        scope: String,
        operation: String,
        #[serde(default)]
        header_value: Option<String>,
    },
    ServiceWorkerSkipWaiting {
        worker_id: u32,
    },
    ServiceWorkerClientsClaim {
        worker_id: u32,
    },
    ServiceWorkerPostMessage {
        scope: String,
        source_origin: String,
        data: serde_json::Value,
        #[serde(default)]
        transfer_ports: Vec<NativeMessagePortTransfer>,
    },
    ServiceWorkerClientPostMessage {
        worker_id: u32,
        client_id: String,
        data: serde_json::Value,
        #[serde(default)]
        transfer_ports: Vec<NativeMessagePortTransfer>,
        #[serde(default)]
        object_urls: Vec<NativeObjectUrlTransfer>,
    },
    ServiceWorkerOpenWindow {
        request_id: u32,
        worker_id: u32,
        url: String,
    },
    ServiceWorkerCacheOpen {
        request_id: u32,
        cache_name: String,
    },
    ServiceWorkerCacheDelete {
        request_id: u32,
        cache_name: String,
    },
    ServiceWorkerCacheHas {
        request_id: u32,
        cache_name: String,
    },
    ServiceWorkerCacheKeys {
        request_id: u32,
    },
    ServiceWorkerCacheMatch {
        request_id: u32,
        cache_name: String,
        request_url: String,
        request_method: String,
        #[serde(default)]
        request_headers: Vec<(String, String)>,
        #[serde(default)]
        ignore_search: bool,
        #[serde(default)]
        ignore_method: bool,
        #[serde(default)]
        ignore_vary: bool,
    },
    ServiceWorkerCachePut {
        request_id: u32,
        cache_name: String,
        request_url: String,
        request_method: String,
        #[serde(default)]
        request_headers: Vec<(String, String)>,
        response: serde_json::Value,
    },
    ServiceWorkerCachePutAll {
        request_id: u32,
        cache_name: String,
        entries: Vec<NativeServiceWorkerCacheBatchEntry>,
    },
    ServiceWorkerCacheDeleteRequest {
        request_id: u32,
        cache_name: String,
        request_url: String,
        request_method: String,
        #[serde(default)]
        request_headers: Vec<(String, String)>,
        #[serde(default)]
        ignore_search: bool,
        #[serde(default)]
        ignore_method: bool,
        #[serde(default)]
        ignore_vary: bool,
    },
    ServiceWorkerCacheEntries {
        request_id: u32,
        cache_name: String,
        #[serde(default)]
        request_url: Option<String>,
        #[serde(default)]
        request_method: Option<String>,
        #[serde(default)]
        request_headers: Vec<(String, String)>,
        #[serde(default)]
        ignore_search: bool,
        #[serde(default)]
        ignore_method: bool,
        #[serde(default)]
        ignore_vary: bool,
    },
    WebSocketOpen {
        socket_id: u32,
        href: String,
        #[serde(default)]
        protocols: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    WebSocketSend {
        socket_id: u32,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        data_base64: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    WebSocketClose {
        socket_id: u32,
        code: u16,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    EventSourceOpen {
        source_id: u32,
        href: String,
        with_credentials: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    EventSourceClose {
        source_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    FetchStreamRead {
        stream_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    FetchStreamCancel {
        stream_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    FetchUploadChunk {
        stream_id: u32,
        data_base64: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    FetchUploadEnd {
        stream_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    FetchUploadError {
        stream_id: u32,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    FetchUploadCancel {
        stream_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
    Dialog {
        dialog_type: String,
        message: String,
        #[serde(default)]
        default_value: Option<String>,
    },
    StorageSet {
        scope: String,
        key: String,
        value: String,
    },
    StorageRemove {
        scope: String,
        key: String,
    },
    StorageClear {
        scope: String,
    },
    CookieSet {
        value: String,
    },
    SetValue {
        node_index: u32,
        value: String,
    },
    ClearFileInput {
        node_index: u32,
    },
    SetSelection {
        node_index: u32,
        start: usize,
        end: usize,
        direction: String,
    },
    CanvasCommit {
        node_index: u32,
        width: u32,
        height: u32,
        pixels_base64: String,
        #[serde(default = "default_true")]
        origin_clean: bool,
    },
    MediaLoad {
        node_index: u32,
    },
    MediaFrameTime {
        node_index: u32,
        time_millis: u64,
        playing: bool,
        playback_rate: f64,
    },
    SetChecked {
        node_index: u32,
        checked: bool,
    },
    SetIndeterminate {
        node_index: u32,
        indeterminate: bool,
    },
    SetSelected {
        node_index: u32,
        selected: bool,
    },
    SetAttribute {
        node_index: u32,
        name: String,
        value: String,
        #[serde(default)]
        namespace_uri: Option<String>,
    },
    RemoveAttribute {
        node_index: u32,
        name: String,
        #[serde(default)]
        namespace_uri: Option<String>,
    },
    SetTextContent {
        node_index: u32,
        value: String,
    },
    SetDocumentTitle {
        value: String,
    },
    SetInnerHtml {
        node_index: u32,
        value: String,
    },
    BindInnerHtmlNodeIndexes {
        root_index: u32,
        temporary_node_indexes: Vec<u32>,
    },
    RemoveNode {
        node_index: u32,
    },
    CreateElement {
        node_index: u32,
        tag_name: String,
        #[serde(default)]
        namespace_uri: Option<String>,
    },
    CreateTextNode {
        node_index: u32,
        value: String,
    },
    CreateComment {
        node_index: u32,
        value: String,
    },
    CreateDocumentType {
        node_index: u32,
        name: String,
        public_id: String,
        system_id: String,
    },
    CreateShadowRoot {
        host_index: u32,
        node_index: u32,
        mode: String,
    },
    AppendChild {
        parent_index: u32,
        child_index: u32,
    },
    InsertBefore {
        parent_index: u32,
        child_index: u32,
        #[serde(default)]
        before_index: Option<u32>,
    },
    /// The JavaScript realm already executed a newly inserted classic inline
    /// script synchronously; the host records the single-shot state without
    /// evaluating it a second time.
    StartScript {
        node_index: u32,
    },
    SetCustomValidity {
        node_index: u32,
        message: String,
    },
    SetFormAssociatedDisabledState {
        node_index: u32,
        disabled: bool,
    },
    CheckValidity {
        node_index: u32,
    },
    ReportValidity {
        node_index: u32,
    },
    /// A same-origin frame construction batch. Parent projections collect
    /// DOM mutations so the child owner can commit temporary node identities
    /// in one transaction.
    FrameScriptBatch {
        commands: Vec<NativeScriptCommand>,
    },
    /// Apply one ordinary DOM command in a same-origin embedded browsing
    /// context. The command is routed by the owning browser topology after
    /// the caller's script returns; it never transfers a native engine or
    /// JavaScript object across realms.
    FrameScript {
        frame_id: String,
        source_frame_id: String,
        command: Box<NativeScriptCommand>,
    },
    /// Relay a command for an adopted node through an accessible frame when
    /// the node's new owner is the caller's own document. FrameScript itself
    /// intentionally targets another context, so the host resolves this
    /// explicit owner before applying the inner DOM operation.
    AdoptedNodeCommand {
        destination_frame_id: String,
        command: Box<NativeScriptCommand>,
    },
    /// Transfer one already modeled DOM subtree between two live owners in
    /// the same browsing-context tree. The browser coordinator validates the
    /// captured generations and commits both document arenas together.
    AdoptNode {
        source_frame_id: String,
        destination_frame_id: String,
        source_generation: u32,
        destination_generation: u32,
        transfers: Vec<NativeNodeSubtreeTransfer>,
    },
    SharedWorkerCreate {
        connection_id: u32,
        href: String,
        name: String,
        #[serde(default = "default_classic_worker_type")]
        worker_type: String,
        #[serde(default = "default_same_origin_credentials")]
        credentials: String,
        #[serde(default)]
        extended_lifetime: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referrer_policy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        constructor_storage_key: Option<NativeSharedWorkerStorageKey>,
        transfer_port: NativeMessagePortTransfer,
    },
    WorkerCreate {
        worker_id: u32,
        href: String,
        #[serde(default)]
        worker_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referrer_policy: Option<String>,
    },
    WorkerPostMessage {
        worker_id: u32,
        data: serde_json::Value,
        #[serde(default)]
        transfer_ports: Vec<NativeMessagePortTransfer>,
        #[serde(default)]
        object_urls: Vec<NativeObjectUrlTransfer>,
    },
    WorkerTerminate {
        worker_id: u32,
    },
    WorkerClose {
        worker_id: u32,
    },
    /// Forward one uncanceled dedicated-worker callback exception to its
    /// owning Worker, ordered with messages emitted by the same event turn.
    WorkerScriptError {
        worker_id: u32,
        message: String,
    },
    MessagePortPostMessage {
        bridge_key: String,
        data: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
        #[serde(default)]
        transfer_ports: Vec<NativeMessagePortTransfer>,
        #[serde(default)]
        object_urls: Vec<NativeObjectUrlTransfer>,
    },
    MessagePortClose {
        bridge_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker_id: Option<u32>,
    },
}
