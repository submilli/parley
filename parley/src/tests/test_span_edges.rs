// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{borrow::ToOwned, string::String};

use super::utils::{
    ColorBrush,
    fonts::{FONT_FAMILY_LIST, create_font_context},
};
use crate::{
    FontFamily, InlineBox, InlineBoxKind, Layout, LayoutContext, TextStyle, TextWrapMode,
    TreeBuilder,
};

fn edge(builder: &mut TreeBuilder<'_, ColorBrush>, id: u64, kind: InlineBoxKind, width: f32) {
    builder.push_inline_box(InlineBox {
        id,
        kind,
        index: 0,
        width,
        height: 0.0,
        baseline: None,
    });
}

fn build(
    wrap: TextWrapMode,
    width: Option<f32>,
    content: impl FnOnce(&mut TreeBuilder<'_, ColorBrush>),
) -> (Layout<ColorBrush>, String) {
    let mut fonts = create_font_context();
    let mut context = LayoutContext::new();
    let style = TextStyle {
        font_family: FontFamily::from(FONT_FAMILY_LIST),
        text_wrap_mode: wrap,
        ..Default::default()
    };
    let mut builder = context.tree_builder(&mut fonts, 1.0, false, &style);
    content(&mut builder);
    let (mut layout, text) = builder.build();
    layout.break_all_lines(width);
    (layout, text)
}

#[test]
fn span_edges_never_make_atomic_wrap_opportunities() {
    let (layout, text) = build(TextWrapMode::Wrap, Some(1.0), |b| {
        b.push_text("ab");
        edge(b, 1, InlineBoxKind::SpanStart, 15.0);
        b.push_text("cd");
        edge(b, 1, InlineBoxKind::SpanEnd, 15.0);
        b.push_text("ef");
    });
    assert_eq!(text, "abcdef");
    assert_eq!(layout.len(), 1);
}

#[test]
fn closing_edge_participates_in_line_fit() {
    let (plain, _) = build(TextWrapMode::Wrap, None, |b| b.push_text("aa bb"));
    let width = plain.width() + 1.0;
    let (layout, text) = build(TextWrapMode::Wrap, Some(width), |b| {
        b.push_text("aa ");
        edge(b, 1, InlineBoxKind::SpanStart, 0.0);
        b.push_text("bb");
        edge(b, 1, InlineBoxKind::SpanEnd, 15.0);
    });
    assert_eq!(text, "aa bb");
    assert_eq!(layout.len(), 2);
    let line = layout.get(1).unwrap();
    assert_eq!(line.text_range(), 3..5);
    assert!(line.items().any(|item| matches!(item, crate::PositionedLayoutItem::InlineBox(box_) if box_.kind == InlineBoxKind::SpanStart)));
}

#[test]
fn empty_decorated_span_keeps_pending_space_in_order() {
    let (_, text) = build(TextWrapMode::Wrap, None, |b| {
        b.push_text("one");
        edge(b, 1, InlineBoxKind::SpanStart, 0.0);
        b.push_text(" ");
        edge(b, 1, InlineBoxKind::SpanEnd, 30.0);
        b.push_text("two");
    });
    assert_eq!(text, "one two");
}

#[test]
fn nested_signed_edges_and_nowrap_preserve_width() {
    let (plain, _) = build(TextWrapMode::NoWrap, None, |b| b.push_text("aa bb"));
    let (layout, _) = build(TextWrapMode::NoWrap, Some(1.0), |b| {
        edge(b, 1, InlineBoxKind::SpanStart, 10.0);
        b.push_text("aa ");
        edge(b, 2, InlineBoxKind::SpanStart, 5.0);
        b.push_text("bb");
        edge(b, 2, InlineBoxKind::SpanEnd, -8.0);
        edge(b, 1, InlineBoxKind::SpanEnd, 3.0);
    });
    assert_eq!(layout.len(), 1);
    assert!((layout.width() - plain.width() - 10.0).abs() < 0.001);
    assert!((layout.calculate_content_widths().max - plain.width() - 10.0).abs() < 0.001);
}

#[test]
fn negative_closing_margin_does_not_undo_last_atom_overflow() {
    let (plain, _) = build(TextWrapMode::Wrap, None, |b| b.push_text("aa bb"));
    let (layout, _) = build(TextWrapMode::Wrap, Some(plain.width() - 5.0), |b| {
        b.push_text("aa ");
        edge(b, 1, InlineBoxKind::SpanStart, 0.0);
        b.push_text("bb");
        edge(b, 1, InlineBoxKind::SpanEnd, -10.0);
    });
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.get(1).unwrap().text_range(), 3..5);
}

#[test]
fn span_edges_follow_the_inside_bidi_run() {
    let (layout, _) = build(TextWrapMode::Wrap, None, |b| {
        b.push_text("abc ");
        edge(b, 1, InlineBoxKind::SpanStart, 7.0);
        b.push_text("אב");
        edge(b, 1, InlineBoxKind::SpanEnd, 9.0);
        b.push_text(" def");
    });
    for item in &layout.data.items {
        if item.kind == crate::layout::data::LayoutItemKind::InlineBox {
            assert!(item.bidi_level.is_rtl());
        }
    }
    let line = layout.get(0).unwrap();
    let edges: std::vec::Vec<_> = line
        .items()
        .filter_map(|item| match item {
            crate::PositionedLayoutItem::InlineBox(box_) => Some(box_),
            _ => None,
        })
        .collect();
    assert_eq!(edges[0].kind, InlineBoxKind::SpanEnd);
    assert_eq!(edges[1].kind, InlineBoxKind::SpanStart);
}

#[test]
fn span_edges_shift_text_carets_without_adding_characters() {
    let (layout, text) = build(TextWrapMode::Wrap, None, |b| {
        edge(b, 1, InlineBoxKind::SpanStart, 10.0);
        b.push_text("ab");
        edge(b, 1, InlineBoxKind::SpanEnd, 20.0);
    });
    assert_eq!(text, "ab");
    let cluster = crate::Cluster::from_byte_index(&layout, 0).unwrap();
    assert_eq!(cluster.visual_offset().unwrap(), 10.0);
    assert!(crate::Cluster::from_byte_index(&layout, 2).is_none());
}

#[test]
fn negative_closing_margin_does_not_undo_earlier_text_overflow() {
    let (layout, _) = build(TextWrapMode::Wrap, Some(30.0), |b| {
        b.push_text("a ");
        edge(b, 1, InlineBoxKind::SpanStart, 0.0);
        b.push_text("longword");
        edge(b, 1, InlineBoxKind::SpanEnd, -100.0);
    });
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.get(0).unwrap().text_range(), 0..2);
    assert_eq!(layout.get(1).unwrap().text_range(), 2..10);
}

#[test]
fn negative_closing_margin_preserves_normal_breaks_inside_the_span() {
    // Chrome's 100px Arial case wraps before "four" even though the closing
    // -30px margin could make the text's combined final advance fit.
    let (first_line, _) = build(TextWrapMode::Wrap, None, |b| b.push_text("one two three"));
    let width = first_line.width() + 1.0;
    let (layout, text) = build(TextWrapMode::Wrap, Some(width), |b| {
        b.push_text("one ");
        edge(b, 1, InlineBoxKind::SpanStart, 0.0);
        b.push_text("two three four");
        edge(b, 1, InlineBoxKind::SpanEnd, -30.0);
        b.push_text(" five");
    });
    let lines: std::vec::Vec<_> = layout
        .lines()
        .map(|line| text[line.text_range()].trim().to_owned())
        .collect();
    assert_eq!(lines, ["one two three", "four five"]);
}

#[test]
fn intrinsic_minimum_keeps_opening_edges_with_text_with_and_without_spacing() {
    for spacing in [0.0, 1.0] {
        let (word, _) = build(TextWrapMode::Wrap, None, |b| {
            b.push_style_modification_span(&[crate::StyleProperty::LetterSpacing(spacing)]);
            b.push_text("longword");
        });
        let expected = word.width() + 40.0;
        let (layout, _) = build(TextWrapMode::Wrap, Some(expected), |b| {
            b.push_style_modification_span(&[crate::StyleProperty::LetterSpacing(spacing)]);
            b.push_text("a ");
            edge(b, 1, InlineBoxKind::SpanStart, 40.0);
            b.push_text("longword");
            edge(b, 1, InlineBoxKind::SpanEnd, 0.0);
        });
        assert!((layout.calculate_content_widths().min - expected).abs() < 0.001);
        assert_eq!(layout.len(), 2);
        assert!((layout.get(1).unwrap().metrics().advance - expected).abs() < 0.001);
    }
}

#[test]
fn closing_decoration_fits_with_terminal_atomic_box() {
    let (prefix, _) = build(TextWrapMode::Wrap, None, |b| b.push_text("one "));
    let (layout, _) = build(TextWrapMode::Wrap, Some(prefix.width() + 69.0), |b| {
        b.push_text("one ");
        edge(b, 1, InlineBoxKind::SpanStart, 0.0);
        b.push_inline_box(InlineBox {
            id: 2,
            kind: InlineBoxKind::InFlow,
            index: 0,
            width: 40.0,
            height: 10.0,
            baseline: None,
        });
        edge(b, 1, InlineBoxKind::SpanEnd, 30.0);
    });
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.calculate_content_widths().min, 70.0);
    assert_eq!(layout.get(1).unwrap().metrics().advance, 70.0);
    let boxes: std::vec::Vec<_> = layout
        .get(1)
        .unwrap()
        .items()
        .filter_map(|item| match item {
            crate::PositionedLayoutItem::InlineBox(box_) => Some(box_.kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        boxes,
        [
            InlineBoxKind::SpanStart,
            InlineBoxKind::InFlow,
            InlineBoxKind::SpanEnd
        ]
    );
}

#[test]
fn closed_empty_span_stays_before_the_following_wrapped_word() {
    let (prefix, _) = build(TextWrapMode::Wrap, None, |b| b.push_text("a "));
    let (layout, _) = build(TextWrapMode::Wrap, Some(prefix.width() + 41.0), |b| {
        b.push_text("a ");
        edge(b, 1, InlineBoxKind::SpanStart, 40.0);
        edge(b, 1, InlineBoxKind::SpanEnd, 0.0);
        b.push_text("longword");
    });
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.get(0).unwrap().text_range(), 0..2);
    assert_eq!(layout.get(1).unwrap().text_range(), 2..10);
    assert_eq!(
        layout
            .get(0)
            .unwrap()
            .items()
            .filter(|item| matches!(item, crate::PositionedLayoutItem::InlineBox(_)))
            .count(),
        2
    );
    assert_eq!(
        layout
            .get(1)
            .unwrap()
            .items()
            .filter(|item| matches!(item, crate::PositionedLayoutItem::InlineBox(_)))
            .count(),
        0
    );
}

#[test]
fn boxes_only_span_line_keeps_the_spans_font_line_extents() {
    let (layout, _) = build(TextWrapMode::Wrap, None, |b| {
        for kind in [
            InlineBoxKind::SpanStart,
            InlineBoxKind::InFlow,
            InlineBoxKind::SpanEnd,
        ] {
            let atomic = kind == InlineBoxKind::InFlow;
            b.push_inline_box(InlineBox {
                id: if atomic { 2 } else { 1 },
                kind,
                index: 0,
                width: if atomic { 40.0 } else { 0.0 },
                height: if atomic { 10.0 } else { 18.0 },
                baseline: if atomic { None } else { Some(14.0) },
            });
        }
    });
    let line = layout.get(0).unwrap();
    assert_eq!(line.metrics().line_height, 18.0);
    assert_eq!(line.metrics().baseline, 14.0);
    let items: std::vec::Vec<_> = line
        .items()
        .filter_map(|item| match item {
            crate::PositionedLayoutItem::InlineBox(box_) => Some(box_),
            _ => None,
        })
        .collect();
    assert_eq!(items[0].y, 0.0);
    assert_eq!(items[1].y, 4.0);
    assert_eq!(items[2].y, 0.0);
}

#[test]
fn span_font_height_yield_resumes_without_consuming_the_child_twice() {
    let (mut layout, _) = build(TextWrapMode::Wrap, None, |b| {
        b.push_inline_box(InlineBox {
            id: 1,
            kind: InlineBoxKind::SpanStart,
            index: 0,
            width: 5.0,
            height: 18.0,
            baseline: Some(14.0),
        });
        b.push_inline_box(InlineBox {
            id: 2,
            kind: InlineBoxKind::InFlow,
            index: 0,
            width: 40.0,
            height: 10.0,
            baseline: None,
        });
        b.push_inline_box(InlineBox {
            id: 1,
            kind: InlineBoxKind::SpanEnd,
            index: 0,
            width: 5.0,
            height: 18.0,
            baseline: Some(14.0),
        });
    });
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_line_max_height(12.0);
    let Some(crate::layout::YieldData::MaxHeightExceeded(yield_)) = breaker.break_next() else {
        panic!("span font extents must yield when the available height is smaller");
    };
    assert_eq!(yield_.line_height, 18.0);
    assert_eq!(yield_.advance, 5.0);
    breaker.state_mut().set_line_max_height(18.0);
    assert!(matches!(
        breaker.break_next(),
        Some(crate::layout::YieldData::LineBreak(_))
    ));
    assert!(breaker.break_next().is_none());
    breaker.finish();
    assert_eq!(layout.get(0).unwrap().metrics().advance, 50.0);
    assert_eq!(layout.get(0).unwrap().metrics().line_height, 18.0);
}

#[test]
fn empty_inner_span_does_not_close_its_outer_owners_opening_checkpoint() {
    let (word, _) = build(TextWrapMode::Wrap, None, |b| b.push_text("longword"));
    let expected = word.width() + 40.0;
    let (layout, _) = build(TextWrapMode::Wrap, Some(expected), |b| {
        b.push_text("a ");
        edge(b, 1, InlineBoxKind::SpanStart, 20.0);
        edge(b, 2, InlineBoxKind::SpanStart, 10.0);
        edge(b, 2, InlineBoxKind::SpanEnd, 5.0);
        b.push_text("longword");
        edge(b, 1, InlineBoxKind::SpanEnd, 5.0);
    });
    assert_eq!(layout.len(), 2);
    assert!((layout.calculate_content_widths().min - expected).abs() < 0.001);
    let edges: std::vec::Vec<_> = layout
        .get(1)
        .unwrap()
        .items()
        .filter_map(|item| match item {
            crate::PositionedLayoutItem::InlineBox(box_) => Some((box_.id, box_.kind)),
            _ => None,
        })
        .collect();
    assert_eq!(
        edges,
        [
            (1, InlineBoxKind::SpanStart),
            (2, InlineBoxKind::SpanStart),
            (2, InlineBoxKind::SpanEnd),
            (1, InlineBoxKind::SpanEnd),
        ]
    );
}
