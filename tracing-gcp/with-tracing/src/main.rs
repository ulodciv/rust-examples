use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::to_string;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Instrument, Subscriber, info, info_span};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, registry};

struct TraceId(String);

#[derive(Default)]
struct TraceIdVisitor {
    trace_id: Option<String>,
}

impl Visit for TraceIdVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "trace_id" {
            self.trace_id = Some(format!("{value:?}"))
        }
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"))
        }
    }
}

struct GcpLayer {
    gcp_project_id: String,
}

impl<S> Layer<S> for GcpLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = TraceIdVisitor::default();
            attrs.record(&mut visitor);
            if let Some(trace_id) = visitor.trace_id {
                span.extensions_mut().insert(TraceId(trace_id));
            }
        };
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut trace = None;
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let Some(trace_id) = extensions.get::<TraceId>() else { continue };
                let t = &trace_id.0;
                trace = Some(format!("projects/{}/traces/{t}", self.gcp_project_id));
            }
        }
        #[derive(Serialize)]
        struct LogEntry {
            severity: String,
            message: String,
            time: String,
            #[serde(rename = "logging.googleapis.com/trace")]
            #[serde(skip_serializing_if = "Option::is_none")]
            trace: Option<String>,
        }
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        let message = visitor.message.unwrap_or_default();
        let severity = event.metadata().level().as_str().to_lowercase();
        let time = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        eprintln!("{}", to_string(&LogEntry { severity, message, time, trace }).unwrap());
    }
}

fn get_gcp_project_id() -> String {
    "PROJECT_ID_123".into()
}

async fn init_logging() {
    let layer = GcpLayer { gcp_project_id: get_gcp_project_id() };
    registry().with(layer.with_filter(LevelFilter::INFO)).init();
}

async fn do_something() {
    info!("Doing something");
    // ...
    info!("Done doing something");
}

#[tokio::main]
async fn main() {
    init_logging().await;

    println!("With trace_id=456");
    do_something().instrument(info_span!("trace_id", trace_id = %"456")).await;

    println!("Without a trace_id:");
    do_something().await;

    println!("With trace_id=789");
    do_something().instrument(info_span!("trace_id", trace_id = %"789")).await;
}
