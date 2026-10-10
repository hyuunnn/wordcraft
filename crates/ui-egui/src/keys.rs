//! Keyboard: typing, editing keys and command shortcuts (from the registry's `shortcut` fields).

use egui::{Key, Modifiers};
use serde_json::{Value, json};

use crate::WordApp;

/// The registry's name for a key.
pub fn key_name(k: Key) -> Option<&'static str> {
    Some(match k {
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Escape => "Escape",
        Key::Space => "Space",
        Key::Equals | Key::Plus => "=",
        Key::Minus => "-",
        Key::Period => ".",
        Key::Comma => ",",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num5 => "5",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::F3 => "F3",
        Key::F5 => "F5",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F12 => "F12",
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        _ => return None,
    })
}

fn combo(m: Modifiers, key: &str, with_shift: bool) -> String {
    let mut s = String::new();
    if m.command {
        s.push_str("Mod+");
    }
    if m.ctrl && !m.command {
        s.push_str("Ctrl+");
    }
    if m.mac_cmd && m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift && with_shift {
        s.push_str("Shift+");
    }
    s.push_str(key);
    s
}

/// Find and run the command bound to this key. Returns true if handled.
fn dispatch(app: &mut WordApp, key: Key, m: Modifiers) -> bool {
    let Some(name) = key_name(key) else { return false };
    let reg = app.session.registry.clone();
    if let Some(spec) = reg.by_shortcut(&combo(m, name, true)) {
        let _ = app.run(spec.id, json!({}));
        return true;
    }
    // Shift extends caret movement.
    if m.shift
        && let Some(spec) = reg.by_shortcut(&combo(m, name, false))
        && spec.id.starts_with("caret.")
    {
        let _ = app.run(spec.id, json!({"extend": true}));
        return true;
    }
    false
}

/// Keys while editing inside an equation. Returns true when handled.
fn equation_key(app: &mut WordApp, key: Key, m: Modifiers) -> bool {
    if m.command || m.alt || (m.ctrl && !cfg!(target_os = "macos")) {
        return false;
    }
    let (id, params) = match key {
        Key::ArrowLeft => ("equation.move", json!({"dir": "left"})),
        Key::ArrowRight => ("equation.move", json!({"dir": "right"})),
        Key::ArrowUp => ("equation.move", json!({"dir": "up"})),
        Key::ArrowDown => ("equation.move", json!({"dir": "down"})),
        Key::Home => ("equation.move", json!({"dir": "home"})),
        Key::End => ("equation.move", json!({"dir": "end"})),
        Key::Tab => ("equation.move", json!({"dir": if m.shift { "prev" } else { "next" }})),
        Key::Backspace => ("equation.backspace", json!({})),
        Key::Delete => ("equation.delete", json!({})),
        Key::Enter => ("equation.enter", json!({})),
        Key::Escape => ("equation.exit", json!({})),
        _ => return false,
    };
    let _ = app.run(id, params);
    true
}

/// Whether releasing `key` with `modifiers` is the plain Paste shortcut (Mod+V or the Paste
/// key) — not Mod+Shift+V (Paste Formatting) or Mod+Alt+V (Paste Special) — so a clipboard
/// picture may be pasted on it (#45).
fn is_paste_release(key: Key, modifiers: egui::Modifiers) -> bool {
    (modifiers.command || key == Key::Paste) && !modifiers.alt && !modifiers.shift
}

/// Events for the focused canvas: text, editing keys, clipboard, IME. While keytips are showing
/// every key belongs to them (letters pick badges, Escape cancels), so nothing here runs.
pub fn canvas_events(app: &mut WordApp, ctx: &egui::Context) {
    if app.ui.keytips != crate::keytips::Phase::Off {
        return;
    }
    crate::ime::validate(app, ctx);
    let mut events: std::collections::VecDeque<_> = ctx.input(|i| i.events.clone()).into();
    let mut cleared_active = false;
    let mut committed = false;
    while let Some(e) = events.pop_front() {
        if let egui::Event::WindowFocused(false) = e {
            crate::ime::cancel(app, ctx);
            continue;
        }
        if let egui::Event::Ime(event) = e {
            match &event {
                egui::ImeEvent::Preedit { text, .. } if !matches!(text.as_str(), "\n" | "\r") => {
                    if text.is_empty() {
                        cleared_active |= crate::ime::active(app);
                    } else {
                        cleared_active = false;
                        committed = false;
                    }
                }
                egui::ImeEvent::Commit(text) if !matches!(text.as_str(), "\n" | "\r") => {
                    if text.is_empty() {
                        cleared_active |= crate::ime::active(app);
                    } else {
                        cleared_active = false;
                    }
                    committed = !text.is_empty();
                }
                _ => {}
            }
            if let Err(e) = crate::ime::event(app, event) {
                app.status(e);
            }
            continue;
        }
        if matches!(e, egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)) {
            match crate::ime::finish_events(app, &mut events) {
                Ok(did_commit) => committed |= did_commit,
                Err(e) => {
                    app.status(e);
                    continue;
                }
            }
        }
        if let egui::Event::Key { key, pressed: true, modifiers, .. } = &e {
            let command = modifiers.command || (modifiers.ctrl && !cfg!(target_os = "macos"));
            let editing = matches!(
                key,
                Key::Enter
                    | Key::Tab
                    | Key::Backspace
                    | Key::Delete
                    | Key::Escape
                    | Key::Space
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown
            );
            let confirmation = *key == Key::Enter
                && (committed
                    || events
                        .iter()
                        .take_while(|e| matches!(e, egui::Event::Ime(_) | egui::Event::Key { pressed: false, .. }))
                        .any(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Commit(t)) if !matches!(t.as_str(), "" | "\n" | "\r"))));
            if !command && editing && (crate::ime::active(app) || cleared_active || confirmation) {
                continue;
            }
            if command || matches!(key, Key::F3 | Key::F5 | Key::F7 | Key::F9 | Key::F12) {
                match crate::ime::finish_events(app, &mut events) {
                    Ok(did_commit) => {
                        committed |= did_commit;
                        cleared_active = false;
                    }
                    Err(e) => {
                        app.status(e);
                        continue;
                    }
                }
            } else {
                crate::ime::dismiss(app);
                committed = false;
            }
        }
        if let egui::Event::Paste(t) = &e {
            app.canvas.pasted = true;
            // Copied picture files arrive as their paths: insert the pictures (#45).
            if app.session.math.is_none() && !ctx.input(|i| i.modifiers.alt) && crate::paste_picture::paste_named_files(app, t) {
                continue;
            }
        }
        // Desktop: Mod+V with no text on the clipboard sends no paste event, only the key's
        // release; paste the clipboard's picture then (#45). A paste event before the release
        // means it was text. (The web reads pasted pictures itself.)
        if let egui::Event::Key { key: key @ (Key::V | Key::Paste), pressed: false, modifiers, .. } = &e {
            let paste_keys = is_paste_release(*key, *modifiers);
            let pasted = std::mem::take(&mut app.canvas.pasted);
            if !pasted && paste_keys && app.session.math.is_none() && app.services.clipboard_picture.is_some() {
                let _ = app.run("edit.paste", json!({}));
            }
            continue;
        }
        // Equation editing uses the same composition lifetime and confirmation-key guard.
        if app.session.math.is_some() {
            match &e {
                egui::Event::Paste(t) => {
                    let _ = app.run("equation.type", json!({"text": t}));
                    continue;
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } if equation_key(app, *key, *modifiers) => continue,
                _ => {}
            }
        }
        match e {
            egui::Event::Text(t) => {
                let m = ctx.input(|i| i.modifiers);
                if m.command || (m.ctrl && !cfg!(target_os = "macos")) {
                    continue;
                }
                if !t.is_empty() && t.chars().all(|c| !c.is_control()) {
                    if let Err(e) = crate::ime::direct_text(app, &t) {
                        app.status(e);
                    }
                    committed = false;
                    cleared_active = false;
                }
            }
            egui::Event::Paste(t) => {
                // Mod+Alt+V arrives as a paste too (the shell reads the clipboard): Paste Special.
                let m = ctx.input(|i| i.modifiers);
                let id = match (m.alt, m.shift) {
                    (true, true) => "edit.pasteText",
                    (true, false) => "edit.pasteSpecial",
                    _ => "edit.paste",
                };
                let _ = app.run(id, json!({"text": t}));
            }
            egui::Event::Copy | egui::Event::Cut => {
                let id = if matches!(e, egui::Event::Cut) { "edit.cut" } else { "edit.copy" };
                if let Ok(r) = app.run(id, json!({}))
                    && let Some(t) = r.get("text").and_then(Value::as_str)
                    && !t.is_empty()
                {
                    ctx.copy_text(t.to_string());
                }
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                if key == Key::Escape && app.session.painter.is_some() {
                    app.session.painter = None;
                    continue;
                }
                // Escape puts the pen or eraser down (back to selecting).
                if key == Key::Escape && app.session.view.draw.mode != wordcraft_engine::cmd::draw::DrawMode::Select {
                    app.canvas.ink = None;
                    let _ = app.run("draw.select", json!({}));
                    continue;
                }
                if key == Key::Escape && crate::table_pen::stop(app) {
                    continue;
                }
                if crate::objects::key(app, key, modifiers) {
                    continue;
                }
                // Escape leaves a text box to just after it in the body.
                if key == Key::Escape
                    && crate::canvas::in_text_box(app)
                    && app.session.sel.is_collapsed()
                    && let wordcraft_doc::StoryRef::Part(id) = app.session.sel.focus.story
                    && let Some(pos) = app.session.doc.text_box_anchor(id)
                {
                    let _ = app.run("caret.set", json!({"pos": pos}));
                    continue;
                }
                if key == Key::Escape && matches!(app.session.sel.focus.story, wordcraft_doc::StoryRef::Part(_)) && app.session.sel.is_collapsed() {
                    let _ = app.run("insert.closeHeader", json!({}));
                    continue;
                }
                dispatch(app, key, modifiers);
            }
            _ => {}
        }
    }
}

/// Command shortcuts when the canvas isn't focused but no text field is either.
pub fn global_shortcuts(app: &mut WordApp, ctx: &egui::Context) {
    if app.canvas.focused || ctx.egui_wants_keyboard_input() {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    for e in events {
        if let egui::Event::Key { key, pressed: true, modifiers, .. } = e
            && (modifiers.command || matches!(key, Key::F3 | Key::F5 | Key::F7 | Key::F9 | Key::F12))
        {
            dispatch(app, key, modifiers);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_paste_releases_paste_a_clipboard_picture() {
        let m = |command, shift, alt| egui::Modifiers { command, shift, alt, ..Default::default() };
        assert!(is_paste_release(Key::V, m(true, false, false)), "Mod+V");
        assert!(is_paste_release(Key::Paste, m(false, false, false)), "the Paste key");
        assert!(!is_paste_release(Key::V, m(true, true, false)), "Mod+Shift+V is Paste Formatting");
        assert!(!is_paste_release(Key::V, m(true, false, true)), "Mod+Alt+V is Paste Special");
        assert!(!is_paste_release(Key::V, m(false, false, false)), "a plain V");
    }
}
