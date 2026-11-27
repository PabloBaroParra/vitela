// The "Edit content" mode as the viewer mounts it (B26 T-258b..T-261): one
// container that puts the mode (`useContentMode`) and its edits
// (`useTextRunEditing`, `useParagraphEditing`, `useTextInsertion`,
// `useImageEditing`) together and hands the pages their pointer input, plus
// the two views the viewer places — the overlay over a page (the selected
// item's, an open paragraph editor, a new line being typed) and the status
// row with Insert text, Insert image and the one image file picker (a
// replacement or an insert).

import { useCallback, useMemo, useRef, type RefObject } from 'react';
import type { DocumentHandle, Engine, PageSize } from '@vitela/core';
import { STAMP_ACCEPT } from '../annotations/stamps';
import type { OpenDocument } from '../document/useDocument';
import { useSaveWaitsFor } from '../save/MarkDirtyContext';
import type { ContentQueue } from './contentEdits';
import type { ContentInput } from './ContentLayer';
import { ContentStatus } from './ContentStatus';
import { ImageOverlay } from './ImageOverlay';
import { TextBlockEditor } from './TextBlockEditor';
import { TextInsertEditor } from './TextInsertEditor';
import { TextRunOverlay, textRunOverlayPage } from './TextRunOverlay';
import { useContentMode } from './useContentMode';
import { useImageEditing } from './useImageEditing';
import { useParagraphEditing } from './useParagraphEditing';
import { useTextInsertion } from './useTextInsertion';
import { useTextRunEditing } from './useTextRunEditing';

interface ContentEditingInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly document: OpenDocument;
  /** Whether the mode is on (the workspace's, already gated). */
  readonly active: boolean;
  /** The viewer's edit queue; a save waits for it, and for an open editor. */
  readonly queue: ContentQueue & { readonly settled: () => Promise<unknown> };
  /** The positions on screen now (no overscan), or `null` before the first layout. */
  readonly visible: { readonly first: number; readonly last: number } | null;
  /** Gives the keyboard back to the pages. */
  readonly focusPages: () => void;
}

export function useContentEditing({ engine, handle, document, active, queue, visible, focusPages }: ContentEditingInput) {
  const { pages, pageIds } = document.info;
  const rotations = useMemo(() => pages.map((page) => page.rotationDegrees), [pages]);
  const content = useContentMode({
    engine,
    handle,
    active,
    pageIds,
    rotations,
    // Reads go by position: none while a step may be putting another page there.
    paused: queue.pending,
    paintRevision: document.paintRevision ?? 0,
    baseGeneration: document.contentBaseGeneration ?? 0,
    visible,
  });
  const textRuns = useTextRunEditing({ engine, handle, info: document.info, content, queue, focusPages });
  const paragraphs = useParagraphEditing({ engine, handle, info: document.info, content, queue, focusPages });
  const insertion = useTextInsertion({ info: document.info, content, queue, focusPages });
  const images = useImageEditing({ engine, handle, info: document.info, content, queue, focusPages });

  // The one file picker of the image edits: a replacement or an insert.
  const imageFile = useRef<HTMLInputElement>(null);
  const openImagePicker = useCallback(() => {
    const input = imageFile.current;
    if (!input) return;
    // The same file picked twice still reports a change.
    input.value = '';
    input.click();
  }, []);

  // A save, or a press beside it, takes what is typed in an open editor first.
  const { commitOpen: commitRun } = textRuns;
  const { commitOpen: commitParagraph } = paragraphs;
  const { commitOpen: commitInsertion } = insertion;
  const commitOpen = useCallback(() => {
    commitRun();
    commitParagraph();
    commitInsertion();
  }, [commitRun, commitParagraph, commitInsertion]);
  const { settled } = queue;
  const settleWithEditor = useCallback(() => {
    commitOpen();
    return settled();
  }, [commitOpen, settled]);
  useSaveWaitsFor(settleWithEditor);

  const { select: contentSelect, hover: contentHover } = content;
  const { open: openRun, move: moveRun, onKey: runKey } = textRuns;
  const { place: placeImage, move: moveImage, onKey: imageKey } = images;
  const { place: placeText, onKey: insertKey } = insertion;
  const { onKey: paragraphKey } = paragraphs;
  const selectedKind = content.selected?.item.kind ?? null;
  const input = useMemo<ContentInput | null>(
    () =>
      content.active
        ? {
            onHover: contentHover,
            onSelect: (pageIndex, point) => {
              // A press beside an open editor commits it (the press keeps the focus where it is).
              commitOpen();
              // An armed insert takes the press: the new line's editor opens there (and
              // takes the keyboard once it mounts), or the image goes there.
              if (placeText(pageIndex, point)) return;
              if (!placeImage(pageIndex, point)) contentSelect(pageIndex, point);
              focusPages();
            },
            // The press before it selected what is there.
            onOpen: () => openRun(),
            onDrag: (_pageIndex, dx, dy) => {
              if (selectedKind === 'image') moveImage(dx, dy);
              else moveRun(dx, dy);
              focusPages();
            },
          }
        : null,
    [content.active, contentHover, contentSelect, commitOpen, focusPages, openRun, moveRun, placeText, placeImage, moveImage, selectedKind],
  );

  /** The keys the pages take for the selected item; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean) => insertKey(key) || paragraphKey(key, shift) || runKey(key, shift) || imageKey(key, shift),
    [insertKey, paragraphKey, runKey, imageKey],
  );

  return { content, textRuns, paragraphs, insertion, images, input, onKey, imageFile, openImagePicker };
}

export type ContentEditing = ReturnType<typeof useContentEditing>;

interface ContentOverlayProps {
  readonly editing: ContentEditing;
  readonly pageIndex: number;
  readonly scale: number;
  readonly pages: readonly Pick<PageSize, 'rotationDegrees'>[];
  readonly focusPages: () => void;
}

/**
 * What is drawn over page `pageIndex`: a new line being typed there, and an
 * open paragraph editor, else the selected item's run editor and toolbar or
 * image handles.
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

/** The selected item's views over page `pageIndex`: the run's editor or toolbar, or the image's handles. */
function SelectedItemOverlay({ editing, pageIndex, scale, pages, focusPages }: ContentOverlayProps) {
  const { textRuns, paragraphs, images, openImagePicker } = editing;
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

interface ContentToolbarProps {
  readonly editing: ContentEditing;
  /** Turns the mode off: its Done button. */
  readonly onDone: () => void;
}

/** The mode's status row, with Insert text, Insert image and the image file picker; nothing while the mode is off. */
export function ContentToolbar({ editing, onDone }: ContentToolbarProps) {
  const { content, insertion, images, imageFile, openImagePicker } = editing;
  return (
    <>
      <ContentStatus
        status={images.prompt ?? insertion.prompt ?? content.status}
        onDone={onDone}
        insertingText={insertion.armed}
        onInsertText={() => {
          if (insertion.armed) return insertion.disarm();
          // One insert armed at a time.
          images.disarm();
          insertion.arm();
        }}
        inserting={images.armed}
        onInsertImage={() => {
          if (images.armed) return images.disarm();
          insertion.disarm();
          images.insert(openImagePicker);
        }}
      />
      {content.active && <ImageFileInput input={imageFile} onFile={images.picked} />}
    </>
  );
}

function ImageFileInput({ input, onFile }: { readonly input: RefObject<HTMLInputElement | null>; readonly onFile: (file: File) => Promise<void> }) {
  return (
    <input
      ref={input}
      type="file"
      className="viewer__image-file"
      accept={STAMP_ACCEPT}
      aria-label="Image file"
      tabIndex={-1}
      hidden
      onChange={(event) => {
        const file = event.currentTarget.files?.[0];
        if (file) void onFile(file);
      }}
    />
  );
}
