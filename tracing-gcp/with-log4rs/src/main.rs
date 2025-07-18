use chrono::{SecondsFormat, Utc};
use log::{LevelFilter, info};
use log4rs::append::console::{ConsoleAppender, Target};
use log4rs::config::{Appender, Config, Root};
use serde::Serialize;
use tokio::task_local;

task_local! {
    static TASK_LOCAL_TRACE_ID: Option<String>;
}

#[derive(Debug)]
struct GcpJsonEncoder {
    gcp_project_id: String,
}

impl log4rs::encode::Encode for GcpJsonEncoder {
    fn encode(
        &self,
        w: &mut dyn log4rs::encode::Write,
        record: &log::Record,
    ) -> anyhow::Result<()> {
        #[derive(Serialize)]
        struct LogEntry {
            severity: String,
            message: String,
            time: String,
            #[serde(rename = "logging.googleapis.com/trace")]
            #[serde(skip_serializing_if = "Option::is_none")]
            trace: Option<String>,
        }
        let severity = record.level().as_str().to_lowercase();
        let time = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let trace_id = TASK_LOCAL_TRACE_ID.try_with(|c| c.clone()).ok().flatten();
        let trace =
            trace_id.map(|t| format!("projects/{}/traces/{t}", self.gcp_project_id));
        let message = format!("{}", record.args());
        let entry = LogEntry { severity, message, time, trace };
        w.write_all(&serde_json::to_vec(&entry).unwrap())?;
        w.write_all("\n".as_bytes())?;
        Ok(())
    }
}

fn get_gcp_project_id() -> String {
    "PROJECT_ID_123".into()
}

async fn init_logging() {
    let stderr = ConsoleAppender::builder()
        .target(Target::Stderr)
        .encoder(Box::new(GcpJsonEncoder { gcp_project_id: get_gcp_project_id() }))
        .build();
    let config = Config::builder()
        .appender(Appender::builder().build("stderr", Box::new(stderr)))
        .build(Root::builder().appender("stderr").build(LevelFilter::Info))
        .unwrap();
    log4rs::init_config(config).unwrap();
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
    TASK_LOCAL_TRACE_ID.scope(Some("456".into()), do_something()).await;

    println!("Without a trace_id:");
    do_something().await;

    println!("With trace_id=789");
    TASK_LOCAL_TRACE_ID.scope(Some("789".into()), do_something()).await;
}
