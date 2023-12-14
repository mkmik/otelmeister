use opentelemetry_proto::tonic::trace::v1::{
    span::{Event, SpanKind},
    status::StatusCode,
    Span, Status,
};
use serde::{Deserialize, Serialize};

use crate::trace::Trace;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JaegerSave {
    data: Vec<JaegerTrace>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JaegerTrace {
    #[serde(rename = "traceID")]
    trace_id: String,
    spans: Vec<JaegerSpan>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JaegerSpan {
    #[serde(rename = "spanID")]
    span_id: String,
    operation_name: String,
    references: Vec<JaegerReference>,
    tags: Vec<JaegerValue>,
    logs: Vec<JaegerLog>,
    start_time: u64, // microseconds
    duration: u64,   // microseconds
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JaegerReference {
    ref_type: String,
    #[serde(rename = "traceID")]
    trace_id: String,
    #[serde(rename = "spanID")]
    span_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JaegerValue {
    key: String,
    #[serde(rename = "type")]
    type_: String,
    value: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JaegerLog {
    timestamp: u64,
    fields: Vec<JaegerValue>,
}

pub fn parse_file(rdr: impl std::io::BufRead) -> anyhow::Result<Trace> {
    let save: JaegerSave = serde_json::from_reader(rdr)?;
    let trace = &save.data[0];
    trace.try_into()
}

impl TryFrom<&JaegerTrace> for Trace {
    type Error = anyhow::Error;

    fn try_from(value: &JaegerTrace) -> Result<Self, Self::Error> {
        let trace_id = hex::decode(&value.trace_id)?;
        let spans = value
            .spans
            .iter()
            .map(|s| s.to_span(trace_id.clone()))
            .collect::<Result<_, _>>()?;
        Ok(Self { spans })
    }
}

impl JaegerSpan {
    fn to_span(&self, trace_id: Vec<u8>) -> anyhow::Result<Span> {
        let parent_span_id = self
            .references
            .iter()
            .find(|&i| i.ref_type == "CHILD_OF")
            .map(|i| hex::decode(&i.span_id))
            .transpose()?
            .unwrap_or_default();

        let start_time_unix_nano = self.start_time * 1000;
        let end_time_unix_nano = start_time_unix_nano + self.duration * 1000;

        let error = self
            .tags
            .iter()
            .find_map(|t| {
                if t.key == "error" && t.type_ == "bool" {
                    t.value.as_bool()
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let status_code = if error {
            StatusCode::Error
        } else {
            StatusCode::Ok
        };
        let status = Status {
            code: status_code.into(),
            ..Default::default()
        };

        let events = self
            .logs
            .iter()
            .flat_map(|s| {
                s.fields
                    .iter()
                    .filter(|f| f.key == "event" && f.type_ == "string")
                    .map(|f| {
                        (
                            s.timestamp,
                            f.value.as_str().unwrap_or_default().to_string(),
                        )
                    })
            })
            .map(|(timestamp, name)| Event {
                time_unix_nano: timestamp * 1000,
                name,
                ..Default::default()
            })
            .collect();

        Ok(Span {
            kind: SpanKind::Internal.into(),
            trace_id,
            span_id: hex::decode(&self.span_id)?,
            name: self.operation_name.clone(),
            parent_span_id,
            start_time_unix_nano,
            end_time_unix_nano,
            status: Some(status),
            events,
            ..Default::default()
        })
    }
}
