// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    test_name,
    util::{ColorBrush, TestEnv},
};
use parley::{
    Alignment, AlignmentOptions, BaseDirection, BreakReason, InlineBox, InlineBoxKind, Layout,
    OverflowWrap, PositionedLayoutItem, StyleProperty, TextWrapMode, WhiteSpaceCollapse, WordBreak,
    YieldData,
};

fn nearly_eq(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
}

/// A layout of `text` with the given whitespace and wrapping modes.
fn build(
    env: &mut TestEnv,
    text: &str,
    collapse: WhiteSpaceCollapse,
    wrap: TextWrapMode,
) -> Layout<ColorBrush> {
    let mut builder = env.ranged_builder(text);
    builder.push_default(StyleProperty::WhiteSpaceCollapse(collapse));
    builder.push_default(StyleProperty::TextWrapMode(wrap));
    builder.build(text)
}

/// The full advance of `text` on a single line, with none of it hanging.
fn advance(env: &mut TestEnv, text: &str) -> f32 {
    let mut builder = env.ranged_builder(text);
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::Preserve,
    ));
    builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
    let mut layout: Layout<ColorBrush> = builder.build(text);
    layout.break_all_lines(None);
    layout.full_width()
}

/// The text of each of `layout`'s lines (none for a line of only inline boxes).
fn line_texts<'a>(layout: &Layout<ColorBrush>, text: &'a str) -> Vec<&'a str> {
    layout
        .lines()
        .map(|line| text.get(line.text_range()).unwrap_or_default())
        .collect()
}

#[test]
fn hanging_across_collapse_mode_boundary() {
    let mut env = TestEnv::new(test_name!(), None);

    // This tests changing `white-space-collapse` inside the trailing whitespace before a forced
    // break. Collapsible whitespace at the end hangs unconditionally, and the preserved whitespace
    // before it also hangs (CSS Text 4 § 4.3.2 says to hang conditionally only if the preserved
    // whitespace sequence is followed by a forced line break).
    //
    // Preserved whitespace at the end hangs conditionally, and the collapsible whitespace before it
    // hangs only if the preserved whitespace hangs in full (CSS Text 4 § 9.2).
    let word = advance(&mut env, "X");
    let space = advance(&mut env, " ");
    let ideographic_space = advance(&mut env, "\u{3000}");
    let whitespace = 2. * space + ideographic_space;
    let full = word + whitespace;
    for (text, collapse_range, conditional) in [
        ("X  \u{3000}\nX", 3..6, false),
        ("X\u{3000}  \nX", 1..4, true),
    ] {
        let mut builder = env.ranged_builder(text);
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        builder.push(
            StyleProperty::WhiteSpaceCollapse(WhiteSpaceCollapse::Collapse),
            collapse_range,
        );
        let mut layout = builder.build(text);
        let widths = layout.calculate_content_widths();
        nearly_eq(widths.min, word);
        nearly_eq(widths.max, if conditional { full } else { word });

        for overflow in [-5., space / 2., whitespace / 2., whitespace + 5.] {
            let width = full - overflow;
            layout.break_all_lines(Some(width));
            assert_eq!(layout.len(), 2, "{text:?} at {width}");
            let line = layout.get(0).unwrap();
            assert_eq!(line.break_reason(), BreakReason::Explicit);
            let hanging = if conditional && overflow < 2. * space {
                overflow.max(0.)
            } else {
                whitespace
            };
            nearly_eq(line.metrics().hanging_advance, hanging);
            nearly_eq(layout.width(), full - hanging);
            // Overflowing lines aren't aligned by default.
            layout.align(Alignment::Right, AlignmentOptions::default());
            nearly_eq(
                layout.get(0).unwrap().metrics().offset,
                (width - full + hanging).max(0.),
            );
        }
    }
}

#[test]
fn overflowing_whitespace_hangs_without_adding_a_break_opportunity() {
    let mut env = TestEnv::new(test_name!(), None);

    // Whitespace that overflows the line hangs, but whether the line breaks after it is up to the
    // line breaking analysis: e.g., there's no opportunity before "!" (UAX #14 LB13).
    //
    // (See `CHROMIUM_LINE_BREAK_OVERRIDE` to break in more positions.)
    let word = advance(&mut env, "aaa");
    for (text, lines) in [("aaa   !", 1), ("aaa   bbb", 2)] {
        let mut layout: Layout<ColorBrush> = env.ranged_builder(text).build(text);
        layout.break_all_lines(Some(word + 1.));
        assert_eq!(layout.len(), lines, "{text:?}");
        let line = layout.get(0).unwrap();
        if lines == 1 {
            assert_eq!(line.break_reason(), BreakReason::None);
        } else {
            assert_eq!(line.break_reason(), BreakReason::Regular);
            nearly_eq(
                line.metrics().hanging_advance,
                advance(&mut env, "aaa   ") - word,
            );
        }
    }
}

#[test]
fn break_spaces_preserves_source_across_spans() {
    let mut env = TestEnv::new(test_name!(), None);
    let source = " \ta  \r\n b\u{2028} \u{3000}";
    for split in source
        .char_indices()
        .map(|(index, _)| index)
        .chain([source.len()])
    {
        let mut builder = env.tree_builder();
        builder.push_style_modification_span(&[StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::BreakSpaces,
        )]);
        builder.push_text(&source[..split]);
        builder.push_style_modification_span(&[StyleProperty::FontSize(24.)]);
        builder.push_text(&source[split..]);
        builder.pop_style_span();
        builder.pop_style_span();
        let (mut layout, text) = builder.build();
        assert_eq!(text, source);
        layout.break_all_lines(None);
        // A style boundary between the `\r` and the `\n` splits the CRLF into two atoms, each of
        // which forces its own break.
        let crlf_split = source[..split].ends_with('\r');
        assert_eq!(layout.len(), if crlf_split { 4 } else { 3 });
        for line in layout.lines() {
            nearly_eq(line.metrics().hanging_advance, 0.);
        }
    }
}

#[test]
fn break_spaces_wraps_each_space_without_hanging() {
    let mut env = TestEnv::new(test_name!(), None);
    let space = advance(&mut env, " ");
    for text in ["a   b", "    "] {
        for width in [0., space + 0.1] {
            let mut layout = build(
                &mut env,
                text,
                WhiteSpaceCollapse::BreakSpaces,
                TextWrapMode::Wrap,
            );
            layout.break_all_lines(Some(width));
            let lines: Vec<_> = layout
                .lines()
                .map(|line| &text[line.text_range()])
                .collect();
            assert_eq!(
                lines,
                if text.starts_with('a') {
                    vec!["a ", " ", " ", "b"]
                } else {
                    vec![" ", " ", " ", " "]
                },
            );
            for line in layout.lines() {
                nearly_eq(line.metrics().hanging_advance, 0.);
            }
            let widths = layout.calculate_content_widths();
            nearly_eq(
                widths.min,
                advance(&mut env, if text.starts_with('a') { "a " } else { " " }),
            );
            nearly_eq(widths.max, advance(&mut env, text));
        }
    }

    // A style span inside the whitespace doesn't move the break opportunities, and `NoWrap`
    // suppresses those of the spaces in its span.
    let text = "a   b";
    for wrap in [TextWrapMode::Wrap, TextWrapMode::NoWrap] {
        let mut builder = env.ranged_builder(text);
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::BreakSpaces,
        ));
        builder.push(StyleProperty::FontSize(24.), 2..3);
        builder.push(StyleProperty::TextWrapMode(wrap), 2..);
        let mut layout = builder.build(text);
        layout.break_all_lines(Some(space + 0.1));
        let lines: Vec<_> = layout
            .lines()
            .map(|line| &text[line.text_range()])
            .collect();
        assert_eq!(
            lines,
            if wrap == TextWrapMode::Wrap {
                vec!["a ", " ", " ", "b"]
            } else {
                vec!["a ", "  b"]
            },
        );
        let max = layout.calculate_content_widths().max;
        layout.break_all_lines(None);
        nearly_eq(layout.full_width(), max);
    }

    // Whitespace in a `Preserve` span still hangs, after breaking at the preceding break-space.
    let mut builder = env.ranged_builder("a   ");
    builder.push_default(StyleProperty::WhiteSpaceCollapse(
        WhiteSpaceCollapse::BreakSpaces,
    ));
    builder.push(
        StyleProperty::WhiteSpaceCollapse(WhiteSpaceCollapse::Preserve),
        2..,
    );
    let mut layout = builder.build("a   ");
    layout.break_all_lines(Some(advance(&mut env, "a ") + 0.1));
    nearly_eq(
        layout.get(0).unwrap().metrics().hanging_advance,
        2. * space - 0.1,
    );
    nearly_eq(
        layout.calculate_content_widths().min,
        advance(&mut env, "a "),
    );
}

#[test]
fn break_spaces_retains_advance_at_forced_breaks_and_block_end() {
    let mut env = TestEnv::new(test_name!(), None);
    for separator in [" ", "\t", "\u{3000}"] {
        let first_line = format!("a{separator}{separator}");
        let full = advance(&mut env, &first_line);
        for ending in ["", "\r\nb", "\u{2029}b"] {
            let text = format!("{first_line}{ending}");
            for wrap in [TextWrapMode::Wrap, TextWrapMode::NoWrap] {
                let mut layout = build(&mut env, &text, WhiteSpaceCollapse::BreakSpaces, wrap);
                layout.break_all_lines(Some(full + 20.));
                assert_eq!(layout.len(), if ending.is_empty() { 1 } else { 2 });
                let line = layout.get(0).unwrap();
                nearly_eq(line.metrics().advance, full);
                nearly_eq(line.metrics().hanging_advance, 0.);
                nearly_eq(layout.calculate_content_widths().max, full);
                layout.align(Alignment::Right, AlignmentOptions::default());
                nearly_eq(layout.get(0).unwrap().metrics().offset, 20.);
                layout.align(Alignment::Center, AlignmentOptions::default());
                nearly_eq(layout.get(0).unwrap().metrics().offset, 10.);
            }
        }
    }
}

#[test]
fn break_spaces_ideographic_space_and_nonbreaking_separators() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = "aa\u{3000}\u{3000}b";
    for word_break in [WordBreak::Normal, WordBreak::BreakAll, WordBreak::KeepAll] {
        let mut builder = env.ranged_builder(text);
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::BreakSpaces,
        ));
        builder.push_default(StyleProperty::WordBreak(word_break));
        let mut layout = builder.build(text);
        layout.break_all_lines(Some(advance(&mut env, "aa") + 0.1));
        let first_line = layout.get(0).unwrap();
        assert_eq!(
            &text[first_line.text_range()],
            if word_break == WordBreak::BreakAll {
                "a"
            } else {
                "aa\u{3000}"
            },
        );
        nearly_eq(first_line.metrics().hanging_advance, 0.);
        layout.break_all_lines(Some(0.));
        nearly_eq(layout.full_width(), layout.calculate_content_widths().min);
    }
    for separator in ["\u{a0}", "\u{2007}", "\u{202f}"] {
        let text = format!("a{separator}{separator}b");
        let mut layout = build(
            &mut env,
            &text,
            WhiteSpaceCollapse::BreakSpaces,
            TextWrapMode::Wrap,
        );
        layout.break_all_lines(Some(0.));
        assert_eq!(layout.len(), 1);
        nearly_eq(layout.get(0).unwrap().metrics().hanging_advance, 0.);
        nearly_eq(layout.calculate_content_widths().min, layout.full_width());
    }
}

#[test]
fn break_spaces_emergency_wrap_and_spacing() {
    let mut env = TestEnv::new(test_name!(), None);
    for overflow_wrap in [
        OverflowWrap::Normal,
        OverflowWrap::Anywhere,
        OverflowWrap::BreakWord,
    ] {
        let text = "a  ";
        let mut builder = env.ranged_builder(text);
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::BreakSpaces,
        ));
        builder.push_default(StyleProperty::OverflowWrap(overflow_wrap));
        builder.push_default(StyleProperty::LetterSpacing(2.));
        builder.push_default(StyleProperty::WordSpacing(3.));
        let mut layout = builder.build(text);
        layout.break_all_lines(Some(0.));
        let first_line = layout.get(0).unwrap();
        assert_eq!(
            &text[first_line.text_range()],
            if overflow_wrap == OverflowWrap::Normal {
                "a "
            } else {
                "a"
            },
        );
        if overflow_wrap != OverflowWrap::BreakWord {
            nearly_eq(layout.full_width(), layout.calculate_content_widths().min);
        }
        let max = layout.calculate_content_widths().max;
        layout.break_all_lines(None);
        nearly_eq(layout.full_width(), max);
    }
}

#[test]
fn break_spaces_inline_boxes_and_rtl() {
    let mut env = TestEnv::new(test_name!(), None);
    let text = "a   b";
    let width = advance(&mut env, " ");
    for direction in [BaseDirection::Ltr, BaseDirection::Rtl] {
        for kind in [
            InlineBoxKind::InFlow,
            InlineBoxKind::OutOfFlow,
            InlineBoxKind::CustomOutOfFlow,
        ] {
            let mut builder = env.ranged_builder(text);
            builder.set_base_direction(direction);
            builder.push_default(StyleProperty::WhiteSpaceCollapse(
                WhiteSpaceCollapse::BreakSpaces,
            ));
            builder.push_inline_box(InlineBox {
                id: 0,
                index: 2,
                width,
                height: 10.,
                baseline: None,
                kind,
            });
            let mut layout = builder.build(text);
            layout.break_all_lines(Some(width + 0.1));
            assert_eq!(
                layout.len(),
                if kind == InlineBoxKind::InFlow { 5 } else { 4 }
            );
            for line in layout.lines() {
                nearly_eq(line.metrics().hanging_advance, 0.);
            }
            nearly_eq(layout.full_width(), layout.calculate_content_widths().min);
        }
    }
}

#[test]
fn whitespace_after_an_overflowing_inline_box_hangs() {
    let mut env = TestEnv::new(test_name!(), None);

    // CSS Text 4 § 4.3.2: the collapsible space after an atomic inline that overflows its line
    // hangs at the end of that line, as after an overflowing word, so the next line starts after
    // it. The box is the line's first content, as it didn't fit after "aa ".
    let width = advance(&mut env, "aa ") + 1.;
    let space = advance(&mut env, " ");
    for (text, lines, hanging) in [
        ("aa  bb", vec!["aa ", " ", "bb"], space),
        ("aa bb", vec!["aa ", "", "bb"], 0.),
    ] {
        let mut builder = env.ranged_builder(text);
        builder.push_inline_box(InlineBox {
            id: 0,
            index: 3,
            width: 3. * width,
            height: 10.,
            baseline: None,
            kind: InlineBoxKind::InFlow,
        });
        let mut layout = builder.build(text);
        layout.break_all_lines(Some(width));
        assert_eq!(line_texts(&layout, text), lines, "{text:?}");
        let line = layout.get(1).unwrap();
        assert_eq!(line.break_reason(), BreakReason::Regular, "{text:?}");
        nearly_eq(line.metrics().hanging_advance, hanging);
        nearly_eq(line.metrics().advance - hanging, 3. * width);
    }
}

#[test]
fn line_breaks_report_their_hanging_advance() {
    let mut env = TestEnv::new(test_name!(), None);

    // A caller placing lines (e.g. beside floats) needs to know how much of a line's advance
    // hangs: the line fits if the rest of it does (CSS Text 4 § 4.3.2).
    let text = "aaa bbb";
    let space = advance(&mut env, " ");
    let mut layout: Layout<ColorBrush> = env.ranged_builder(text).build(text);
    let width = advance(&mut env, "aaa") + space / 2.;
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(width);
    breaker.state_mut().set_line_max_advance(width);
    let mut hanging = Vec::new();
    while let Some(yielded) = breaker.break_next() {
        if let YieldData::LineBreak(line) = yielded {
            hanging.push(line.hanging_advance);
        }
    }
    breaker.finish();
    assert_eq!(hanging.len(), 2);
    nearly_eq(hanging[0], space);
    nearly_eq(hanging[1], 0.);
    for (line, hanging) in layout.lines().zip(hanging) {
        nearly_eq(line.metrics().hanging_advance, hanging);
    }
}

#[test]
fn trailing_whitespace_takes_the_paragraph_level() {
    let mut env = TestEnv::new(test_name!(), None);

    // UAX #9 L1 resets the white space at the end of a line to the paragraph level, so in a
    // right-to-left paragraph the space ending a wrapped left-to-right run sits at the line's
    // (left) end edge and hangs there, and the run is flush with the start (right) edge.
    let text = "aaa aaa";
    let word = advance(&mut env, "aaa");
    let space = advance(&mut env, " ");
    let width = word + space / 2.;
    let mut builder = env.ranged_builder(text);
    builder.set_base_direction(BaseDirection::Rtl);
    let mut layout: Layout<ColorBrush> = builder.build(text);
    layout.break_all_lines(Some(width));
    layout.align(Alignment::Start, AlignmentOptions::default());
    assert_eq!(line_texts(&layout, text), ["aaa ", "aaa"]);
    let line = layout.get(0).unwrap();
    nearly_eq(line.metrics().hanging_advance, space);
    let runs: Vec<_> = line
        .items()
        .filter_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => {
                Some((&text[run.run().text_range()], run.offset(), run.advance()))
            }
            PositionedLayoutItem::InlineBox(_) => None,
        })
        .collect();
    assert_eq!(runs.len(), 2, "{runs:?}");
    // In visual order: the hanging space, then the word ending at the right edge.
    assert_eq!(runs[0].0, " ", "{runs:?}");
    assert_eq!(runs[1].0, "aaa", "{runs:?}");
    nearly_eq(runs[1].1 + runs[1].2, width);
    nearly_eq(runs[0].1 + runs[0].2, runs[1].1);
    // In visual order, as above: the space is right-to-left like the paragraph.
    let directions: Vec<_> = line
        .runs()
        .map(|run| (&text[run.text_range()], run.is_rtl()))
        .collect();
    assert_eq!(directions, [(" ", true), ("aaa", false)]);

    // The min-content width excludes the hanging space as well.
    nearly_eq(layout.calculate_content_widths().min, word);
}

/// The text and direction of `line`'s runs, in visual order.
fn visual_runs<'a>(line: &parley::Line<'_, ColorBrush>, text: &'a str) -> Vec<(&'a str, bool)> {
    line.runs()
        .map(|run| (&text[run.text_range()], run.is_rtl()))
        .collect()
}

#[test]
fn trailing_whitespace_of_a_right_to_left_run_hangs_at_a_left_to_right_lines_end() {
    let mut env = TestEnv::new(test_name!(), None);

    // The space between two Hebrew words is right-to-left, but ends the line once it wraps there:
    // UAX #9 L1 resets it to the paragraph's level, so it hangs at the line's right end, past
    // the Hebrew word, also when the line is justified.
    let text = "abc \u{5d0}\u{5d1}\u{5d2} \u{5d0}\u{5d1}\u{5d2}";
    let first = advance(&mut env, "abc \u{5d0}\u{5d1}\u{5d2}");
    let space = advance(&mut env, " ");
    let width = first + space / 2.;
    for alignment in [Alignment::Start, Alignment::Justify] {
        let mut layout: Layout<ColorBrush> = env.ranged_builder(text).build(text);
        layout.break_all_lines(Some(width));
        layout.align(alignment, AlignmentOptions::default());
        let line = layout.get(0).unwrap();
        nearly_eq(line.metrics().hanging_advance, space);
        assert_eq!(
            visual_runs(&line, text),
            [
                ("abc ", false),
                ("\u{5d0}\u{5d1}\u{5d2}", true),
                (" ", false)
            ],
            "{alignment:?}"
        );
        let content_end = line
            .items()
            .filter_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => Some(run),
                PositionedLayoutItem::InlineBox(_) => None,
            })
            .nth(1)
            .map(|run| run.offset() + run.advance())
            .unwrap();
        let expected = if alignment == Alignment::Justify {
            width
        } else {
            first
        };
        nearly_eq(content_end, expected);
    }
}

#[test]
fn tabs_and_the_whitespace_before_them_take_the_paragraph_level() {
    let mut env = TestEnv::new(test_name!(), None);

    // UAX #9 L1 resets a segment separator (a tab) and the white space before it to the paragraph
    // level wherever it is, so a tab splits a left-to-right run in a right-to-left paragraph,
    // whose parts then read from right to left. A no-break space is not white space to L1.
    for (text, runs) in [
        (
            "aaa\tbbb",
            vec![("bbb", false), ("\t", true), ("aaa", false)],
        ),
        (
            "aaa  \tbbb",
            vec![("bbb", false), ("  \t", true), ("aaa", false)],
        ),
        (
            "aaa \u{a0}\tbbb",
            vec![("bbb", false), ("\t", true), ("aaa \u{a0}", false)],
        ),
    ] {
        let mut builder = env.ranged_builder(text);
        builder.set_base_direction(BaseDirection::Rtl);
        builder.push_default(StyleProperty::WhiteSpaceCollapse(
            WhiteSpaceCollapse::Preserve,
        ));
        let mut layout: Layout<ColorBrush> = builder.build(text);
        layout.break_all_lines(None);
        assert_eq!(layout.len(), 1, "{text:?}");
        assert_eq!(visual_runs(&layout.get(0).unwrap(), text), runs, "{text:?}");
    }
}

#[test]
fn line_end_whitespace_is_reset_by_its_bidi_class() {
    let mut env = TestEnv::new(test_name!(), None);

    // UAX #9 L1 resets white space (class `WS`) ending a line, but not a narrow no-break space
    // (`CS`), and it ignores a zero width space (`BN`, removed by rule X9) among the white space.
    let space = advance(&mut env, " ");
    for (text, content, first_line, runs) in [
        (
            "aaa\u{202f} aaa",
            "aaa\u{202f}",
            "aaa\u{202f} ",
            // The no-break space, shaped as a run of its own, stays left-to-right after "aaa".
            vec![(" ", true), ("aaa", false), ("\u{202f}", false)],
        ),
        (
            "aaa \u{200b}aaa",
            "aaa",
            "aaa \u{200b}",
            vec![(" \u{200b}", true), ("aaa", false)],
        ),
    ] {
        let width = advance(&mut env, content) + space / 2.;
        let mut builder = env.ranged_builder(text);
        builder.set_base_direction(BaseDirection::Rtl);
        let mut layout: Layout<ColorBrush> = builder.build(text);
        layout.break_all_lines(Some(width));
        assert_eq!(line_texts(&layout, text)[0], first_line, "{text:?}");
        assert_eq!(visual_runs(&layout.get(0).unwrap(), text), runs, "{text:?}");
    }
}
