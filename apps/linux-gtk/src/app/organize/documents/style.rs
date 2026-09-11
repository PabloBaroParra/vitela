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
"#;
