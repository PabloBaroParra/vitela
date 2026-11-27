// What the "Edit content" mode draws over one page (B26 T-258b..T-263): a new
// line being typed there, an open paragraph editor, else the selected item's
// views — a run's editor or toolbar, an image's or a path's handles and
// toolbar. Presentational wiring of `useContentEditing`'s containers onto
// their views; the viewer mounts it inside that page's slot. Every busy flag
// it hands down already counts a cut being read (`clipping`).

import type { PageSize } from '@vitela/core';
import type { ContentEditing } from './ContentEditing';
import { ImageOverlay } from './ImageOverlay';
import { PathOverlay } from './PathOverlay';
import { TextBlockEditor } from './TextBlockEditor';
import { TextInsertEditor } from './TextInsertEditor';
import { TextRunOverlay, textRunOverlayPage } from './TextRunOverlay';

interface ContentOverlayProps {
  readonly editing: ContentEditing;
  readonly pageIndex: number;
  readonly scale: number;
  readonly pages: readonly Pick<PageSize, 'rotationDegrees'>[];
  readonly focusPages: () => void;
}

/**
 * What is drawn over page `pageIndex`: a new line being typed there, and an
 * open paragraph editor, else the selected item's run editor and toolbar, or
 * an image's or a path's handles and toolbar.
 */
export function ContentOverlay({ editing, pageIndex, scale, pages, focusPages }: ContentOverlayProps) {
  const { insertion, paragraphs } = editing;
  const rotationDegrees = pages[pageIndex]?.rotationDegrees ?? 0;
  const inserting = insertion.editor?.pageIndex === pageIndex ? insertion.editor : null;
  const paragraph = paragraphs.editor;
  return (
    <>
      {inserting && (
        <TextInsertEditor
          key={`${inserting.pageKey}@${inserting.at.x},${inserting.at.y}`}
          editor={inserting}
          scale={scale}
          rotationDegrees={rotationDegrees}
          onCommit={insertion.commit}
          onCancel={insertion.cancel}
          onType={insertion.typing}
          onAttach={insertion.attach}
          onDetach={insertion.detach}
        />
      )}
      {paragraph ? (
        paragraph.pageIndex === pageIndex && (
          <TextBlockEditor
            key={`${paragraph.pageKey}#${paragraph.block.runIds.join(',')}`}
            editor={paragraph}
            scale={scale}
            rotationDegrees={rotationDegrees}
            onCommit={paragraphs.commit}
            onCancel={paragraphs.cancel}
            onType={paragraphs.typing}
            onAttach={paragraphs.attach}
            onDetach={paragraphs.detach}
          />
        )
      ) : (
        <SelectedItemOverlay editing={editing} pageIndex={pageIndex} scale={scale} pages={pages} focusPages={focusPages} />
      )}
    </>
  );
}

/** The selected item's views over page `pageIndex`: the run's editor or toolbar, or the image's or path's handles. */
function SelectedItemOverlay({ editing, pageIndex, scale, pages, focusPages }: ContentOverlayProps) {
  const { textRuns, paragraphs, images, paths, openImagePicker } = editing;
  if (textRunOverlayPage(textRuns) === pageIndex) {
    return (
      <TextRunOverlay
        textRuns={textRuns}
        scale={scale}
        pages={pages}
        focusPages={focusPages}
        onEditParagraph={paragraphs.paragraph ? paragraphs.open : undefined}
      />
    );
  }
  if (paths.path?.pageIndex === pageIndex) {
    return (
      <PathOverlay
        path={paths.path}
        scale={scale}
        busy={paths.busy}
        onResize={(rect) => {
          paths.resize(rect);
          focusPages();
        }}
        onRemove={() => {
          paths.remove();
          focusPages();
        }}
      />
    );
  }
  if (images.image?.pageIndex !== pageIndex) return null;
  return (
    <ImageOverlay
      image={images.image}
      scale={scale}
      busy={images.busy}
      replaceable={images.replaceable}
      replaceRefusal={images.replaceRefusal}
      onResize={(rect) => {
        images.resize(rect);
        focusPages();
      }}
      onReplace={() => images.replace(openImagePicker)}
      onRemove={() => {
        images.remove();
        focusPages();
      }}
    />
  );
}
