use clap::{Parser, ValueEnum};
use opentelemetry_proto::tonic::trace::v1::Span;
use prost::Message;
use std::fmt::{Display, Formatter};
use trace::Trace;

mod jaeger;
mod trace;
pub mod view;

type Result<T> = anyhow::Result<T>;

#[derive(Debug, Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Jaeger,
    OTEL,
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.to_possible_value()
            .ok_or(std::fmt::Error)?
            .get_name()
            .fmt(f)
    }
}

fn open_trace(path: &patharg::InputArg, format: &Format) -> Result<Trace> {
    let file = path.open()?;
    match format {
        Format::Jaeger => jaeger::parse_file(file),
        Format::OTEL => todo!(),
    }
}

#[derive(Clone, Parser)]
pub struct ConvertCmd {
    #[arg(short = 'f', long = "input-file", default_value_t)]
    in_file: patharg::InputArg,

    #[arg(short = 'w', long = "output-file", default_value_t)]
    out_file: patharg::OutputArg,

    #[arg(short = 'i', long = "input-format", default_value_t = Format::Jaeger)]
    in_format: Format,

    #[arg(short = 'o', long = "output-format", default_value_t = Format::OTEL)]
    out_format: Format,
}

impl ConvertCmd {
    pub fn run(&self) -> Result<()> {
        let trace = open_trace(&self.in_file, &self.in_format)?;
        for span in trace.spans {
            println!("{}", serde_json::to_string(&span)?);
        }
        Ok(())
    }
}

pub fn foo() {
    let span = Span {
        trace_id: vec![0, 1, 2, 3, 4, 5, 6, 7],
        span_id: vec![0, 1, 2, 3, 4, 5, 6, 7],
        dropped_attributes_count: 42,
        ..Default::default()
    };

    // Serialize the message to the buffer
    let v = span.encode_to_vec();

    println!("{:?}", v);

    println!("{}", serde_json::to_string(&span).unwrap());
}
