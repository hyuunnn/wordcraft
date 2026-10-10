//! Real egui frames: IME events must reach the canvas across composition updates.

use egui::{Event, ImeEvent, Key, Modifiers};
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::Session;

use crate::{Services, WordApp};

fn app() -> WordApp {
    WordApp::new(Session::new(Document::new()), Services::default())
}

fn frame(ctx: &egui::Context, app: &mut WordApp, events: Vec<Event>) -> egui::FullOutput {
    let time = ctx.input(|i| i.time) + 0.1;
    let input = egui::RawInput {
        events,
        time: Some(time),
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 900.0))),
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    })
}

fn ready(ctx: &egui::Context, app: &mut WordApp) {
    for _ in 0..4 {
        frame(ctx, app, vec![]).drop_without_applying_deltas();
    }
}

fn preedit(text: &str) -> Event {
    Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: None })
}

fn commit(text: &str) -> Event {
    Event::Ime(ImeEvent::Commit(text.into()))
}

fn key(key: Key) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

fn text(app: &WordApp) -> String {
    app.session.doc.plain_text(StoryRef::Body)
}

#[test]
fn korean_composition_keeps_canvas_focus_and_commits_once() {
    let ctx = egui::Context::default();
    let mut app = app();
    ready(&ctx, &mut app);
    for candidate in ["ㅇ", "아", "안", "안녕", "안녕하세요"] {
        let out = frame(&ctx, &mut app, vec![preedit(candidate)]);
        assert!(app.canvas.focused, "canvas lost focus at {candidate}");
        assert!(out.platform_output.ime.is_some(), "native IME disabled at {candidate}");
        assert_eq!(app.canvas.ime_preedit, candidate);
        assert_eq!(text(&app), "", "preedit must not enter the saved document");
        out.drop_without_applying_deltas();
    }
    frame(&ctx, &mut app, vec![preedit(""), commit("안녕하세요")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "안녕하세요");
    app.run("edit.undo", serde_json::json!({})).unwrap();
    assert_eq!(text(&app), "");
}

#[test]
fn korean_composition_owns_backspace_and_confirmation_enter() {
    let ctx = egui::Context::default();
    let mut app = app();
    app.run("text.insert", serde_json::json!({"text": "앞"})).unwrap();
    ready(&ctx, &mut app);
    frame(&ctx, &mut app, vec![preedit("안")]).drop_without_applying_deltas();
    frame(&ctx, &mut app, vec![key(Key::Backspace), preedit("아")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "앞");
    frame(&ctx, &mut app, vec![preedit(""), commit("아"), key(Key::Enter)]).drop_without_applying_deltas();
    assert_eq!(text(&app), "앞아", "IME confirmation must not create a paragraph");
    frame(&ctx, &mut app, vec![key(Key::Enter)]).drop_without_applying_deltas();
    assert_eq!(text(&app), "앞아\n", "the next ordinary Enter must still work");
}

#[test]
fn interface_fonts_cover_korean_preedit_without_system_fonts() {
    if wordcraft_fonts::korean_fonts().next().is_none() {
        assert!(std::env::var_os("CRAFT_FONTS_REQUIRED").is_none(), "prepare the Korean fallback before release builds");
        return;
    }
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::font_definitions(false, false));
    ctx.run_ui(egui::RawInput::default(), |ui| {
        for font in [crate::theme::regular(16.0), crate::theme::medium(16.0), crate::theme::semibold(16.0), egui::FontId::monospace(16.0)] {
            // Compare the rendered glyphs: has_glyph can report a replacement square as present.
            let rasterized = |text: &str| {
                let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE));
                galley.rows.iter().flat_map(|row| &row.glyphs).filter(|g| !g.uv_rect.is_nothing()).map(|g| g.uv_rect).collect::<Vec<_>>()
            };
            let replacement = rasterized("\u{10ffff}");
            for ch in "안녕하세요 한글 ㅇㅏ한".chars().filter(|c| !c.is_whitespace()) {
                let glyphs = rasterized(&ch.to_string());
                assert!(!glyphs.is_empty() && glyphs != replacement, "missing {ch} in {font:?}");
            }
            assert_eq!(rasterized("한글"), rasterized("한글"), "decomposed Hangul must form syllables");
        }
    })
    .drop_without_applying_deltas();
}

#[test]
fn empty_preedit_does_not_swallow_ordinary_keys() {
    let ctx = egui::Context::default();
    let mut app = app();
    ready(&ctx, &mut app);
    frame(&ctx, &mut app, vec![preedit(""), key(Key::Enter)]).drop_without_applying_deltas();
    assert_eq!(text(&app), "\n");
    frame(&ctx, &mut app, vec![preedit("안"), preedit("")]).drop_without_applying_deltas();
    frame(&ctx, &mut app, vec![key(Key::Enter)]).drop_without_applying_deltas();
    assert_eq!(text(&app), "\n\n");
    frame(&ctx, &mut app, vec![commit("새")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "\n\n새");
}

#[test]
fn backspace_that_dismisses_preedit_keeps_committed_text() {
    for end in [preedit(""), commit("")] {
        let ctx = egui::Context::default();
        let mut app = app();
        app.run("text.insert", serde_json::json!({"text": "앞"})).unwrap();
        ready(&ctx, &mut app);
        frame(&ctx, &mut app, vec![preedit("ㅇ")]).drop_without_applying_deltas();
        frame(&ctx, &mut app, vec![end, key(Key::Backspace)]).drop_without_applying_deltas();
        assert_eq!(text(&app), "앞");
        frame(&ctx, &mut app, vec![key(Key::Backspace)]).drop_without_applying_deltas();
        assert_eq!(text(&app), "");
    }
}

#[test]
fn confirmation_before_commit_and_physical_letter_keys_preserve_composition() {
    let ctx = egui::Context::default();
    let mut app = app();
    ready(&ctx, &mut app);
    frame(&ctx, &mut app, vec![preedit("ㅇ")]).drop_without_applying_deltas();
    frame(&ctx, &mut app, vec![key(Key::S), preedit("안")]).drop_without_applying_deltas();
    assert_eq!(app.canvas.ime_preedit, "안");
    frame(&ctx, &mut app, vec![key(Key::Enter), preedit(""), commit("안")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "안");
}

#[test]
fn shortcut_uses_queued_candidate_and_local_commit_is_not_duplicated() {
    for queued in [false, true] {
        let ctx = egui::Context::default();
        let mut app = app();
        ready(&ctx, &mut app);
        frame(&ctx, &mut app, vec![preedit("안")]).drop_without_applying_deltas();
        let mut events = vec![Event::Key { key: Key::B, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }];
        if queued {
            events.extend([preedit(""), commit("安")]);
        }
        frame(&ctx, &mut app, events).drop_without_applying_deltas();
        let expected = if queued { "安" } else { "안" };
        assert_eq!(text(&app), expected);
        if !queued {
            frame(&ctx, &mut app, vec![preedit(""), commit("안")]).drop_without_applying_deltas();
            assert_eq!(text(&app), "안", "native echo after a local finish inserts nothing");
        }
        frame(&ctx, &mut app, vec![preedit("안"), commit("안")]).drop_without_applying_deltas();
        assert_eq!(text(&app), format!("{expected}안"), "a fresh identical syllable is still accepted");
    }
}

#[test]
fn moved_caret_and_replaced_document_reject_late_commit() {
    for replace in [false, true] {
        let ctx = egui::Context::default();
        let mut app = app();
        app.run("text.insert", serde_json::json!({"text": "앞뒤"})).unwrap();
        ready(&ctx, &mut app);
        frame(&ctx, &mut app, vec![preedit("안"), preedit("")]).drop_without_applying_deltas();
        if replace {
            app.session.run("file.new", &serde_json::json!({})).unwrap();
        } else {
            app.session.run("caret.left", &serde_json::json!({})).unwrap();
        }
        frame(&ctx, &mut app, vec![commit("안"), commit("녕")]).drop_without_applying_deltas();
        assert_eq!(text(&app), if replace { "" } else { "앞뒤" });
        frame(&ctx, &mut app, vec![preedit("새"), commit("새")]).drop_without_applying_deltas();
        assert_eq!(text(&app), if replace { "새" } else { "앞새뒤" });
    }
}

#[test]
fn save_finishes_last_syllable_and_round_trips_it() {
    let ctx = egui::Context::default();
    let mut app = app();
    ready(&ctx, &mut app);
    frame(&ctx, &mut app, vec![commit("한"), preedit("글")]).drop_without_applying_deltas();
    let path = std::env::temp_dir().join(format!("wordcraft-ime-{}.docx", std::process::id()));
    app.run("file.saveAs", serde_json::json!({"path": path})).unwrap();
    assert_eq!(text(&app), "한글");
    frame(&ctx, &mut app, vec![preedit(""), commit("글")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "한글");
    app.session.run("file.open", &serde_json::json!({"path": path})).unwrap();
    assert_eq!(text(&app), "한글");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cancelled_window_composition_does_not_type_into_a_new_target() {
    let ctx = egui::Context::default();
    let mut app = app();
    ready(&ctx, &mut app);
    frame(&ctx, &mut app, vec![preedit("안")]).drop_without_applying_deltas();
    frame(&ctx, &mut app, vec![Event::WindowFocused(false)]).drop_without_applying_deltas();
    frame(&ctx, &mut app, vec![preedit(""), commit("안")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "");
}

#[test]
fn korean_commit_types_into_equations_and_text_boxes() {
    for command in ["insert.equation", "insert.textBox"] {
        let ctx = egui::Context::default();
        let mut app = app();
        app.run(command, serde_json::json!({})).unwrap();
        let story = app.session.sel.focus.story;
        ready(&ctx, &mut app);
        frame(&ctx, &mut app, vec![preedit("한"), key(Key::Backspace), preedit("하")]).drop_without_applying_deltas();
        frame(&ctx, &mut app, vec![preedit(""), commit("한글"), key(Key::Enter)]).drop_without_applying_deltas();
        assert_eq!(app.session.sel.focus.story, story);
        if command == "insert.equation" {
            let m = app.session.math.as_ref().expect("confirmation must not exit the equation");
            let object = app.session.doc.para_at(&m.at).unwrap().object_at(m.at.off).unwrap();
            assert_eq!(object.plain_text(), "한글");
        } else {
            assert_eq!(app.session.doc.plain_text(story), "한글");
        }
    }
}

#[test]
fn save_shortcut_includes_queued_final_syllable() {
    let ctx = egui::Context::default();
    let mut app = app();
    ready(&ctx, &mut app);
    let path = std::env::temp_dir().join(format!("wordcraft-ime-shortcut-{}.docx", std::process::id()));
    app.run("file.saveAs", serde_json::json!({"path": path})).unwrap();
    frame(&ctx, &mut app, vec![preedit("한")]).drop_without_applying_deltas();
    frame(
        &ctx,
        &mut app,
        vec![Event::Key { key: Key::S, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }, preedit(""), commit("韓")],
    )
    .drop_without_applying_deltas();
    assert!(!app.session.dirty);
    app.session.run("file.open", &serde_json::json!({"path": path})).unwrap();
    assert_eq!(text(&app), "韓", "save must use final candidate conversion rather than the preview");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn action_after_external_caret_move_resets_native_ime_and_rejects_stale_text() {
    let ctx = egui::Context::default();
    let mut app = app();
    app.run("text.insert", serde_json::json!({"text": "앞뒤"})).unwrap();
    ready(&ctx, &mut app);
    frame(&ctx, &mut app, vec![preedit("안")]).drop_without_applying_deltas();
    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 900.0))), ..Default::default() };
    let out = ctx.run_ui(input, |ui| {
        app.session.run("caret.left", &serde_json::json!({})).unwrap();
        app.run("format.bold", serde_json::json!({})).unwrap();
        app.logic(ui.ctx());
        app.ui(ui);
    });
    assert!(out.platform_output.ime.as_ref().unwrap().should_interrupt_composition);
    out.drop_without_applying_deltas();
    frame(&ctx, &mut app, vec![commit("안")]).drop_without_applying_deltas();
    assert_eq!(text(&app), "앞뒤");
}
