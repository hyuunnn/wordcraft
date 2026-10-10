//! Transient canvas composition. Only the final text goes through the engine's typing command.

use std::collections::VecDeque;

use egui::{Event, ImeEvent};
use serde_json::json;
use wordcraft_engine::{MathEdit, Selection, Session};

use crate::WordApp;

#[derive(Clone)]
struct Target {
    document: u64,
    revision: u64,
    selection: Selection,
    math: Option<MathEdit>,
}

impl Target {
    fn current(s: &Session) -> Self {
        Self { document: s.document_id(), revision: s.rev(), selection: s.sel.clone(), math: s.math.clone() }
    }

    fn same_position(&self, s: &Session) -> bool {
        self.document == s.document_id() && self.selection == s.sel && self.math == s.math
    }

    fn matches(&self, s: &Session) -> bool {
        self.same_position(s) && self.revision == s.rev()
    }
}

#[derive(Default)]
pub(crate) struct State {
    // Retain the target when an empty preedit precedes Commit (winit's normal ordering).
    target: Option<Target>,
    cancelled: bool,
    // A locally finished composition can be echoed once by the native input method.
    retired: Option<(Target, String)>,
}

pub(crate) fn active(app: &WordApp) -> bool {
    !app.canvas.ime_preedit.is_empty() && !app.canvas.ime.cancelled
}

pub(crate) fn cancel(app: &mut WordApp, ctx: &egui::Context) {
    if app.canvas.ime.target.is_some() && !app.canvas.ime.cancelled {
        app.canvas.ime.cancelled = true;
        app.canvas.ime_preedit.clear();
        ctx.memory_mut(|m| m.interrupt_ime());
    }
}

pub(crate) fn validate(app: &mut WordApp, ctx: &egui::Context) {
    if app.canvas.ime.target.as_ref().is_some_and(|t| !t.matches(&app.session)) {
        cancel(app, ctx);
    }
}

/// An ordinary key after a dismissed preedit starts a fresh input session.
pub(crate) fn dismiss(app: &mut WordApp) {
    if app.canvas.ime_preedit.is_empty() && !app.canvas.ime.cancelled {
        app.canvas.ime.target = None;
        app.canvas.ime.retired = None;
    }
}

fn insert(app: &mut WordApp, text: &str) -> Result<(), String> {
    if app.session.math.is_some() {
        app.run("equation.type", json!({"text": text}))?;
    } else {
        crate::objects::enter_text_box(app);
        app.run("text.insert", json!({"text": text}))?;
    }
    Ok(())
}

pub(crate) fn direct_text(app: &mut WordApp, text: &str) -> Result<(), String> {
    if !active(app) {
        app.canvas.ime = State::default();
        insert(app, text)?;
    }
    Ok(())
}

pub(crate) fn event(app: &mut WordApp, event: ImeEvent) -> Result<(), String> {
    match event {
        ImeEvent::Preedit { text, .. } => {
            if matches!(text.as_str(), "\n" | "\r") {
                return Ok(());
            }
            if text.len() > 1 << 20 {
                return Err("IME composition exceeds 1 MiB".into());
            }
            if !text.is_empty() {
                if !app.canvas.ime.cancelled && app.canvas.ime.target.as_ref().is_some_and(|t| !t.matches(&app.session)) {
                    return Ok(());
                }
                if app.canvas.ime.target.is_none() || app.canvas.ime.cancelled {
                    crate::objects::enter_text_box(app);
                    app.canvas.ime.target = Some(Target::current(&app.session));
                }
                app.canvas.ime.cancelled = false;
                app.canvas.ime.retired = None;
            }
            app.canvas.ime_preedit = text;
        }
        ImeEvent::Commit(text) => {
            if matches!(text.as_str(), "\n" | "\r") {
                return Ok(());
            }
            if text.len() > 1 << 20 {
                return Err("IME composition exceeds 1 MiB".into());
            }
            if let Some((target, previous)) = app.canvas.ime.retired.take()
                && (text == previous || !target.same_position(&app.session))
            {
                return Ok(());
            }
            let stale = app.canvas.ime.cancelled || app.canvas.ime.target.take().is_some_and(|t| !t.matches(&app.session));
            app.canvas.ime_preedit.clear();
            // Keep cancellation until fresh preedit/direct text: multiple late events must
            // never leak into a different document, selection, or text field.
            if !stale && !text.is_empty() {
                insert(app, &text)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Finish before Save, formatting, clipboard and other application actions, with echo protection.
pub(crate) fn finish(app: &mut WordApp) -> Result<bool, String> {
    let Some(target) = app.canvas.ime.target.take() else { return Ok(false) };
    let text = std::mem::take(&mut app.canvas.ime_preedit);
    if let Some(ctx) = &app.ctx {
        ctx.memory_mut(|m| m.interrupt_ime());
    }
    if app.canvas.ime.cancelled || !target.matches(&app.session) {
        app.canvas.ime.cancelled = true;
        return Ok(false);
    }
    if text.is_empty() {
        // A queued commit may still belong here, but must be rejected after a document switch.
        app.canvas.ime.retired = Some((target, text));
        return Ok(false);
    }
    insert(app, &text)?;
    app.canvas.ime.retired = Some((Target::current(&app.session), text));
    Ok(true)
}

/// CJK conversion can make Commit differ from Preedit; use the queued final result when present.
pub(crate) fn finish_events(app: &mut WordApp, events: &mut VecDeque<Event>) -> Result<bool, String> {
    if app.canvas.ime.target.is_none() {
        return Ok(false);
    }
    let commit = events
        .iter()
        .take_while(|e| matches!(e, Event::Ime(_) | Event::Key { pressed: false, .. }))
        .position(|e| matches!(e, Event::Ime(ImeEvent::Commit(t)) if !matches!(t.as_str(), "\n" | "\r")));
    if let Some(index) = commit {
        let mut committed = false;
        for _ in 0..=index {
            if let Some(Event::Ime(ime)) = events.pop_front() {
                committed |= matches!(&ime, ImeEvent::Commit(t) if !t.is_empty());
                event(app, ime)?;
            }
        }
        Ok(committed)
    } else {
        finish(app)
    }
}
