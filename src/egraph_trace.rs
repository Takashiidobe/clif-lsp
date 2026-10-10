use std::cell::RefCell;
use std::sync::Once;

use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use log::{LevelFilter, Log, Metadata, Record};

use crate::stages::{self, Target};

#[derive(Clone, Debug)]
pub struct Entry {
    pub target: String,
    pub message: String,
}

thread_local! {
    static CAPTURE: RefCell<Option<Vec<Entry>>> = const { RefCell::new(None) };
}

struct Capture;

fn wanted(target: &str) -> bool {
    target.starts_with("cranelift_codegen::egraph") || target.starts_with("cranelift_codegen::opts")
}

impl Log for Capture {
    fn enabled(&self, metadata: &Metadata) -> bool {
        wanted(metadata.target())
    }

    fn log(&self, record: &Record) {
        if !wanted(record.target()) {
            return;
        }
        CAPTURE.with(|slot| {
            if let Some(entries) = slot.borrow_mut().as_mut() {
                entries.push(Entry { target: record.target().to_owned(), message: record.args().to_string() });
            }
        });
    }

    fn flush(&self) {}
}

static INSTALL: Once = Once::new();

fn install() {
    INSTALL.call_once(|| {
        let _ = log::set_logger(&Capture);
    });
}

struct Level;

impl Level {
    fn raise() -> Level {
        log::set_max_level(LevelFilter::Trace);
        Level
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        log::set_max_level(LevelFilter::Off);
        CAPTURE.with(|slot| slot.borrow_mut().take());
    }
}

pub fn capture(target: &Target) -> Result<Vec<Entry>, String> {
    install();
    let mut context = Context::for_function(target.function.clone());
    let guard = Level::raise();
    CAPTURE.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let result = context.optimize(&*target.isa, &mut ControlPlane::default());
    let entries = CAPTURE.with(|slot| slot.borrow_mut().take()).unwrap_or_default();
    drop(guard);
    result.map_err(|e| stages::describe(&e))?;
    Ok(entries)
}
