use clap::Parser;
use duration_human::DurationHuman;
use opentelemetry_proto::tonic::trace::v1::status::StatusCode;
use std::{cell::Cell, collections::HashMap, rc::Rc};

use crate::{open_trace, trace::Trace, Format};

type Result<T> = anyhow::Result<T>;

#[derive(Clone, Parser)]
pub struct ViewCmd {
    #[arg(short = 'f', long = "input-file", default_value_t)]
    file: patharg::InputArg,

    #[arg(short = 'i', long = "input-format", default_value_t = Format::Jaeger)]
    format: Format,
}

#[derive(Default, Clone)]
struct Children {
    spans: Vec<Span>,
}

#[derive(Default, Clone)]
struct Span {
    span: opentelemetry_proto::tonic::trace::v1::Span,
    children: Rc<Cell<Children>>,
}

impl std::fmt::Debug for Children {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Children")
            .field(
                "spans",
                &self
                    .spans
                    .iter()
                    .map(|Span { span, .. }| span)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

fn build_tree(trace: &Trace) -> Result<Children> {
    let mut children_map: HashMap<Vec<u8>, Rc<Cell<Children>>> = HashMap::new();

    for span in &trace.spans {
        let children = Children::default();
        children_map.insert(span.span_id.clone(), Rc::new(Cell::new(children)));
    }

    for span in &trace.spans {
        let child = Span {
            span: span.clone(),
            children: children_map.get(&span.span_id).unwrap().clone(),
        };

        if let Some(parent_cell) = children_map.get(&span.parent_span_id) {
            let mut parent_children = parent_cell.clone().take();
            parent_children.spans.push(child);
            parent_cell.set(parent_children);
        }
    }

    let mut root_nodes: Vec<Span> = vec![];

    for span in &trace.spans {
        if children_map.get(&span.parent_span_id).is_none() {
            root_nodes.push(Span {
                span: span.clone(),
                children: children_map.get(&span.span_id).unwrap().clone(),
            });
        }
    }

    Ok(Children { spans: root_nodes })
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len && max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}

fn gray(s: &str) -> String {
    format!("\x1b[38;5;255m{s}\x1b[0m")
}

#[derive(Debug)]
struct Renderer {
    w_names: usize,
    w_durations: usize,
    min_start_time: u64,
    total_duration: u64,
}
impl Renderer {
    fn render(&self, children: &Children, indent: usize) {
        for item in &children.spans {
            let (error, w_names) = if item
                .span
                .status
                .as_ref()
                .is_some_and(|s| s.code == StatusCode::Error as i32)
            {
                ("\x1b[1;38;5;160m\u{24D8}  \x1b[0m", self.w_names - 3)
            } else {
                ("", self.w_names)
            };
            let indented = format!("{:indent$}{}", "", item.span.name);
            let trimmed = truncate(&indented, w_names - 2);

            let scale = |n: u64| (n * self.w_durations as u64 / self.total_duration) as usize;

            let span_pos = scale(item.span.start_time_unix_nano - self.min_start_time);
            let span_width = scale(item.span.end_time_unix_nano - item.span.start_time_unix_nano);
            let human_duration =
                DurationHuman::new(item.span.end_time_unix_nano - item.span.start_time_unix_nano)
                    .to_string();
            let human_duration = gray(&human_duration);

            let (left_duration, right_duration) = if span_pos > self.w_durations / 2 {
                (human_duration + " ", "".to_string())
            } else {
                ("".to_string(), " ".to_string() + &human_duration)
            };

            // very short times get a lighter shade gray
            let (span_width, fill_char) = if span_width > 0 {
                (span_width, "\u{2593}")
            } else {
                (1, "\u{2592}")
            };

            let bar = format!(
                "{left_duration:>span_pos$}{}{right_duration}",
                fill_char.repeat(span_width)
            );

            println!(
                "> {error}{trimmed}{:pad$} \u{23D0} {bar}",
                "",
                pad = w_names - trimmed.len(),
            );

            for event in &item.span.events {
                println!(
                    "_ {:pad$} \u{23D0} {event}",
                    "",
                    event = gray(&format!(
                        "{event} (+{time})",
                        time = DurationHuman::new(
                            event.time_unix_nano - item.span.start_time_unix_nano
                        ),
                        event = &event.name
                    )),
                    pad = self.w_names
                );
            }

            self.render(&item.children.clone().take(), indent + 1);
        }
    }
}

impl ViewCmd {
    pub fn run(&self) -> anyhow::Result<()> {
        let trace = open_trace(&self.file, &self.format)?;
        let trace = trace.sorted();

        let window_size = crossterm::terminal::window_size()?;

        let min_start_time = trace
            .spans
            .iter()
            .map(|s| s.start_time_unix_nano)
            .min()
            .unwrap();
        let max_end_time = trace
            .spans
            .iter()
            .map(|s| s.end_time_unix_nano)
            .max()
            .unwrap();
        let w_names = 40usize;
        let renderer = Renderer {
            min_start_time,
            total_duration: max_end_time - min_start_time,
            w_names,
            w_durations: (window_size.columns as usize)
                .checked_sub(w_names)
                .unwrap()
                .checked_sub(2)
                .unwrap(),
        };

        let tree = build_tree(&trace)?;
        renderer.render(&tree, 0);

        Ok(())
    }
}
