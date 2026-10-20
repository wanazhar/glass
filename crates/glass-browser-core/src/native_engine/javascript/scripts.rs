//! Embedded JavaScript entry programs grouped by browser responsibility.

#[path = "scripts/canvas.rs"]
mod canvas;
pub(super) use canvas::NATIVE_CANVAS_SCRIPT;
#[path = "scripts/font_face.rs"]
mod font_face;
pub(super) use font_face::NATIVE_FONT_FACE_SCRIPT;
#[path = "scripts/timers.rs"]
mod timers;
pub(super) use timers::NATIVE_PAGE_TIMER_DELAY_SOURCE;
pub(super) use timers::NATIVE_WORKER_TIMER_DELAY_SOURCE;
#[path = "scripts/worker_bootstrap.rs"]
mod worker_bootstrap;
pub(super) use worker_bootstrap::NATIVE_DYNAMIC_IMPORT_OPTIONS_BOOTSTRAP;
pub(super) use worker_bootstrap::NATIVE_SHARED_WORKER_BOOTSTRAP;
#[path = "scripts/message_channel.rs"]
mod message_channel;
pub(super) use message_channel::NATIVE_MESSAGE_CHANNEL_BOOTSTRAP;
#[path = "scripts/service_worker_page.rs"]
mod service_worker_page;
pub(super) use service_worker_page::NATIVE_SERVICE_WORKER_PAGE_SCRIPT;
#[path = "scripts/xml_document.rs"]
mod xml_document;
pub(super) use xml_document::NATIVE_XML_DOCUMENT_SCRIPT;
#[path = "scripts/service_worker.rs"]
mod service_worker;
pub(super) use service_worker::NATIVE_SERVICE_WORKER_BOOTSTRAP;
