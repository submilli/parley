// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// A box to be laid out inline with text
#[derive(PartialEq, Debug, Clone)]
pub struct InlineBox {
    /// User-specified identifier for the box, which can be used by the user to determine which box in
    /// parley's output corresponds to which box in its input.
    ///
    /// For span edges this also identifies the owner: a `SpanStart` and its matching
    /// `SpanEnd` must have the same id. Distinct spans that are open at the same time
    /// must use distinct ids. Supply paired edges in logical order with proper nesting
    /// (outer start, inner start, inner end, outer end); crossing pairs are unsupported.
    /// For coincident text offsets, preserve this order when inserting the edges.
    pub id: u64,
    /// Whether the box is in-flow (takes up space in the layout) or out-of-flow (e.g. absolutely positioned or floated)
    pub kind: InlineBoxKind,
    /// The byte offset into the underlying text string at which the box should be placed.
    /// This must not be within a Unicode code point.
    pub index: usize,
    /// The horizontal advance in pixels, including any margins.
    ///
    /// This may be negative for negative margins.
    pub width: f32,
    /// The height of the box in pixels.
    ///
    /// For a span edge, this is the span's font line-height, excluding decoration padding.
    /// A span edge with height zero and no baseline contributes no vertical metrics.
    pub height: f32,
    /// The baseline of the box in pixels, relative to the top of the box
    ///
    /// If `None`, the baseline is the box's bottom edge.
    pub baseline: Option<f32>,
}

/// Whether a box is in-flow (takes up space in the layout) or out-of-flow (e.g. absolutely positioned)
/// or custom-out-of-flow (line-breaking should yield control flow)
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum InlineBoxKind {
    /// `InFlow` boxes take up space in the layout and flow in line with text
    ///
    /// They correspond to `display: inline-block` boxes in CSS.
    InFlow,
    /// The opening horizontal decoration of a non-atomic inline span.
    ///
    /// It contributes signed width and the span's supplied font line-box extents,
    /// without adding a text character or wrap opportunity.
    /// Use edges only for spans with horizontal decorations: unlike plain style spans, they
    /// count as content during whitespace processing, even if their widths are filled in later.
    SpanStart,
    /// The closing horizontal decoration of a non-atomic inline span.
    ///
    /// Like `SpanStart`, it remains attached to the span's content when lines wrap.
    SpanEnd,
    /// `OutOfFlow` boxes are assigned a position as if they were a zero-sized inline box, but
    /// do not take up space in the layout.
    ///
    /// They correspond to `position: absolute` boxes in CSS.
    OutOfFlow,
    /// `CustomOutOfFlow` boxes also do not take up space in the layout, but they are not assigned a position
    /// by Parley. When they are encountered, control flow is yielded back to the caller who is then responsible
    /// for laying out the box.
    ///
    /// They can be used to implement advanced layout modes such as CSS's `float`
    CustomOutOfFlow,
}

impl InlineBoxKind {
    /// Whether the item contributes horizontal advance to the line.
    pub fn is_in_flow(self) -> bool {
        matches!(self, Self::InFlow | Self::SpanStart | Self::SpanEnd)
    }

    /// Whether the item is a decoration edge of a non-atomic span.
    pub fn is_span_edge(self) -> bool {
        matches!(self, Self::SpanStart | Self::SpanEnd)
    }
}
