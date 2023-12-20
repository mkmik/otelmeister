// This file is a bit of a mess because I let gpt-4 write the bulk of it
// (with a ton of manual assistence)

use clap::Parser;
use duration_human::DurationHuman;
use opentelemetry_proto::tonic::trace::v1::{status::StatusCode, Span};
use std::{cell::Cell, collections::HashMap, hash::Hash, rc::Rc};

use crate::{open_trace, Format};

type Result<T> = anyhow::Result<T>;

#[derive(Clone, Parser)]
pub struct ViewCmd {
    #[arg(short = 'f', long = "input-file", default_value_t)]
    file: patharg::InputArg,

    #[arg(short = 'i', long = "input-format", default_value_t = Format::Jaeger)]
    format: Format,
}

trait Spanlike {
    type SpanID;

    fn span_id(&self) -> Self::SpanID;
    fn parent_span_id(&self) -> Self::SpanID;
}

impl Spanlike for Span {
    type SpanID = Vec<u8>;

    fn span_id(&self) -> Self::SpanID {
        self.span_id.clone().clone()
    }

    fn parent_span_id(&self) -> Self::SpanID {
        self.parent_span_id.clone()
    }
}

#[derive(Default, Clone)]
struct Node<S>
where
    S: Spanlike,
{
    span: S,
    children: Rc<Cell<Vec<Node<S>>>>,
}

fn build_tree<S>(spans: &[S]) -> Result<Vec<Node<S>>>
where
    S: Spanlike + Clone + Default,
    <S as Spanlike>::SpanID: Eq + Hash + Clone,
{
    let mut children_map = HashMap::new();

    for span in spans {
        let children = Default::default();
        children_map.insert(span.span_id().clone(), Rc::new(Cell::new(children)));
    }

    for span in spans {
        let child = Node {
            span: span.clone(),
            children: children_map.get(&span.span_id()).unwrap().clone(),
        };

        if let Some(parent_cell) = children_map.get(&span.parent_span_id()) {
            let mut parent_children = parent_cell.clone().take();
            parent_children.push(child);
            parent_cell.set(parent_children);
        }
    }

    let mut root_nodes: Vec<Node<S>> = vec![];

    for span in spans {
        if children_map.get(&span.parent_span_id()).is_none() {
            root_nodes.push(Node {
                span: span.clone(),
                children: children_map.get(&span.span_id()).unwrap().clone(),
            });
        }
    }

    Ok(root_nodes)
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
    fn render(&self, children: &[Node<Span>], indent: usize) {
        for item in children {
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

        let tree = build_tree(&trace.spans)?;
        renderer.render(&tree, 0);

        Ok(())
    }
}
