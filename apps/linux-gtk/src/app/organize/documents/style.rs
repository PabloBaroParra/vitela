//! The Organize screen's stylesheet — both views of it.
//!
//! Its own module rather than a constant at the top of [`super`]: sixty
//! lines of CSS between two functions is the kind of thing that makes a
//! module look bigger than the behaviour it holds, and nothing here is read
//! by anything but `shell::install_shell_css`.

pub(crate) const ORGANIZE_CSS: &str = r#"
.organize-view-switch button {
  min-height: 30px;
  padding: 4px 16px;
  color: #625b72;
}

.organize-view-switch button:checked {
  background: #f2edff;
  color: #302d3a;
  font-weight: 600;
}

.organize-card,
.organize-block {
  background: #ffffff;
  border: 1px solid #e3e0e9;
  border-radius: 10px;
  padding: 10px;
}

.organize-block:focus-visible {
  outline: 2px solid #6b4eff;
  outline-offset: -2px;
}

.organize-thumb,
.organize-cover {
  background: #e9e6ec;
  border-radius: 6px;
}

/* The two sheets peeking out behind the cover. Purely decorative: they are
   empty boxes, never a render of the block's other pages. */
.organize-cover-sheet {
  background: #f1eef6;
  border: 1px solid #e3e0e9;
  border-radius: 6px;
}

.organize-block-name {
  font-weight: 600;
  color: #302d3a;
}

.organize-block-meta {
  font-size: 0.85em;
  color: #625b72;
}

/* A drop position between two block cards. It keeps its height when idle so
   the list does not jump as the pointer crosses it — only the colour
   changes, and only for the one position the block would land in. */
.organize-gap {
  min-height: 12px;
  border-radius: 4px;
}

.organize-gap-active {
  background: #6b4eff;
}

/* A page card's provenance line — the PDF its page came from, shown only
   once the document holds pages from more than one. Quiet enough to be read
   after the thumbnail and the page number, never before them. */
.organize-card-source {
  font-size: 0.78em;
  color: #625b72;
}

/* The Pages grid's equivalent of `.organize-gap-active`: the insertion slot
   drawn on the near edge of the card the dragged page would land beside.
   An inset shadow rather than a border, so lighting it up never changes the
   card's size and re-flows the grid mid-drag. */
.organize-card-drop-before {
  box-shadow: inset 3px 0 0 0 #6b4eff;
}

.organize-card-drop-after {
  box-shadow: inset -3px 0 0 0 #6b4eff;
}

/* The entrance the cards of a view make when the user switches to it
   (checklist §11). Opacity, scale and displacement together: a card lifts
   the last few pixels into its place instead of blinking into existence,
   which is what makes the pages read as coming *out of* the blocks they
   were just part of.

   `backwards` fill is what makes the stagger work — without it a card with
   a 220ms delay is drawn at full opacity for those 220ms and then jumps
   back to the start of its own animation.

   `organize::motion` adds these classes, and only ever on a view switch;
   nothing here runs on an undo or a preview refresh. GTK4 skips CSS
   animations entirely when `gtk-enable-animations` is off, and the module
   declines to add the classes at all in that case — belt and braces, since
   only one of the two is testable without a frame clock. */
@keyframes organize-enter {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.organize-enter {
  animation-name: organize-enter;
  animation-duration: 180ms;
  animation-timing-function: ease-out;
  animation-fill-mode: backwards;
}

/* One rule per stagger slot, because GTK4 CSS has no inline styles. The
   count here is the cap on the whole sequence — see `organize::motion`. */
.organize-enter-0 {
  animation-delay: 0ms;
}

.organize-enter-1 {
  animation-delay: 20ms;
}

.organize-enter-2 {
  animation-delay: 40ms;
}

.organize-enter-3 {
  animation-delay: 60ms;
}

.organize-enter-4 {
  animation-delay: 80ms;
}

.organize-enter-5 {
  animation-delay: 100ms;
}

.organize-enter-6 {
  animation-delay: 120ms;
}

.organize-enter-7 {
  animation-delay: 140ms;
}

.organize-enter-8 {
  animation-delay: 160ms;
}

.organize-enter-9 {
  animation-delay: 180ms;
}

.organize-enter-10 {
  animation-delay: 200ms;
}

.organize-enter-11 {
  animation-delay: 220ms;
}
"#;
