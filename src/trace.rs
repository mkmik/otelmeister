use opentelemetry_proto::tonic::trace::v1::Span;

pub struct Trace {
    pub spans: Vec<Span>,
}

impl Trace {
    pub fn sorted(&self) -> Self {
        let mut spans = self.spans.clone();
        spans.sort_by_key(|s| s.start_time_unix_nano);
        Self { spans }
    }
}
