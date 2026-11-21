// The tool registry: every PDF tool Vitela has, in the order the home page
// lists them. One entry per tool is the single source of truth for the home
// grid, the tool routes and the hosting rewrites (public/_redirects).
//
// `web` says whether this build can do the job. Porting a tool to the web is
// flipping its `web` to true once the engine supports it.
//
// Icons are stroke paths on a 24×24 grid with round caps and joins, most of
// them taken from the shared shell icons in assets/icons.

import type { DocumentScreen } from '@vitela/viewer';

export type ToolId =
  | 'view'
  | 'merge'
  | 'split'
  | 'organize'
  | 'rotate'
  | 'compress'
  | 'edit'
  | 'annotate'
  | 'forms'
  | 'watermark'
  | 'sign'
  | 'protect'
  | 'pdf-to-image';

export type ToolCategory = 'view' | 'organize' | 'optimize' | 'edit' | 'security' | 'convert';

/** Named hues for the icon tile; each maps to `--tone-<name>-bg/fg` tokens. */
export type ToolTone = 'blue' | 'green' | 'amber' | 'rose' | 'violet';

export interface Tool {
  readonly id: ToolId;
  /** Where the tool lives, e.g. `/edit`. Needs a rewrite in public/_redirects. */
  readonly path: string;
  readonly name: string;
  /** One plain sentence. */
  readonly description: string;
  readonly category: ToolCategory;
  readonly tone: ToolTone;
  /** SVG path data (`d`) of each stroke. */
  readonly icon: readonly string[];
  /** True when this web build can do it; false while it is app-only. */
  readonly web: boolean;
  /** The screen an opened document starts on; the viewer when absent. */
  readonly screen?: DocumentScreen;
}

/** Filter order on the home page. */
export const CATEGORIES: readonly { readonly id: ToolCategory; readonly label: string }[] = [
  { id: 'view', label: 'View' },
  { id: 'organize', label: 'Organize' },
  { id: 'optimize', label: 'Optimize' },
  { id: 'edit', label: 'Edit' },
  { id: 'security', label: 'Security' },
  { id: 'convert', label: 'Convert' },
];

const PAGE = 'M6.9 3.5h10.2a2 2 0 0 1 2 2v13a2 2 0 0 1-2 2H6.9a2 2 0 0 1-2-2v-13a2 2 0 0 1 2-2Z';

export const TOOLS: readonly Tool[] = [
  {
    id: 'view',
    path: '/view',
    name: 'View PDF',
    description: 'Read any PDF, find and copy its text, and save a copy. Password-protected files work too.',
    category: 'view',
    tone: 'blue',
    icon: ['M5.5 3.5h9l4 4v13h-13z', 'M14.5 3.5v4h4', 'M8.5 12.5h7M8.5 16h5'],
    web: true,
  },
  {
    id: 'merge',
    path: '/merge',
    name: 'Merge PDF',
    description: 'Combine several PDFs into one file, in the order you choose.',
    category: 'organize',
    tone: 'green',
    icon: ['M3.5 5h3.2a2 2 0 0 1 1.7.9L12 12', 'M3.5 19h3.2a2 2 0 0 0 1.7-.9L12 12h8.5', 'm17 8.5 3.5 3.5-3.5 3.5'],
    web: false,
  },
  {
    id: 'split',
    path: '/split',
    name: 'Split PDF',
    description: 'Break a document into separate files by page range.',
    category: 'organize',
    tone: 'green',
    icon: [
      'M4.9 9.5v-4a2 2 0 0 1 2-2h10.2a2 2 0 0 1 2 2v4',
      'M4.9 14.5v4a2 2 0 0 0 2 2h10.2a2 2 0 0 0 2-2v-4',
      'M3.5 12h2M9 12h2M13 12h2M18.5 12h2',
    ],
    web: false,
  },
  {
    id: 'organize',
    path: '/organize',
    name: 'Organize pages',
    description: 'Reorder, rotate and delete pages, with undo.',
    category: 'organize',
    tone: 'green',
    icon: [
      'M5 3.5h4A1.5 1.5 0 0 1 10.5 5v4A1.5 1.5 0 0 1 9 10.5H5A1.5 1.5 0 0 1 3.5 9V5A1.5 1.5 0 0 1 5 3.5Z',
      'M15 3.5h4A1.5 1.5 0 0 1 20.5 5v4a1.5 1.5 0 0 1-1.5 1.5h-4A1.5 1.5 0 0 1 13.5 9V5A1.5 1.5 0 0 1 15 3.5Z',
      'M5 13.5h4a1.5 1.5 0 0 1 1.5 1.5v4A1.5 1.5 0 0 1 9 20.5H5A1.5 1.5 0 0 1 3.5 19v-4A1.5 1.5 0 0 1 5 13.5Z',
      'M15 13.5h4a1.5 1.5 0 0 1 1.5 1.5v4a1.5 1.5 0 0 1-1.5 1.5h-4a1.5 1.5 0 0 1-1.5-1.5v-4a1.5 1.5 0 0 1 1.5-1.5Z',
    ],
    web: true,
    screen: 'organize',
  },
  {
    id: 'rotate',
    path: '/rotate',
    name: 'Rotate pages',
    description: 'Turn pages a quarter at a time, one page or the whole file.',
    category: 'organize',
    tone: 'green',
    icon: ['M20.5 12a8.5 8.5 0 1 1-8.5-8.5c2.38 0 4.66.94 6.36 2.59L20.5 8.22', 'M20.5 3.5v4.72h-4.72'],
    web: false,
  },
  {
    id: 'compress',
    path: '/compress',
    name: 'Compress PDF',
    description: 'Make large files smaller so they are easier to send.',
    category: 'optimize',
    tone: 'amber',
    icon: ['M3.5 12h17', 'M12 3.5v5.6', 'M8.7 5.8 12 9.1l3.3-3.3', 'M12 20.5v-5.6', 'M8.7 18.2 12 14.9l3.3 3.3'],
    web: false,
  },
  {
    id: 'edit',
    path: '/edit',
    name: 'Edit PDF',
    description: 'Change text and images where they sit on the page.',
    category: 'edit',
    tone: 'violet',
    icon: ['M3.5 20.5l1.2-4.9L15.6 4.3a2.1 2.1 0 0 1 3 0l1.1 1.1a2.1 2.1 0 0 1 0 3L8.4 19.3Z', 'M14.6 5.3 18.7 9.4'],
    web: false,
  },
  {
    id: 'annotate',
    path: '/annotate',
    name: 'Annotate PDF',
    description: 'Highlight text, add notes and draw on the page.',
    category: 'edit',
    tone: 'violet',
    icon: ['m9 11 8-8 4 4-8 8Z', 'm9 11-3 5 2 2 5-3M6 16l-3 3h6M3 22h18'],
    web: false,
  },
  {
    id: 'forms',
    path: '/forms',
    name: 'Fill forms',
    description: 'Type into the fields of a PDF form and tick its boxes.',
    category: 'edit',
    tone: 'violet',
    icon: [
      'M5 3.5h14a1.5 1.5 0 0 1 1.5 1.5v3a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 8V5A1.5 1.5 0 0 1 5 3.5Z',
      'M5 14.5h3.5A1.5 1.5 0 0 1 10 16v3a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 19v-3A1.5 1.5 0 0 1 5 14.5Z',
      'm5.4 17.6 1.1 1.1 1.9-2.1',
      'M13.5 17.5h7',
    ],
    web: false,
  },
  {
    id: 'watermark',
    path: '/watermark',
    name: 'Add watermark',
    description: 'Stamp text or a picture across every page.',
    category: 'edit',
    tone: 'violet',
    icon: [PAGE, 'm8.5 15.5 7-7', 'm11 17 4.5-4.5'],
    web: false,
  },
  {
    id: 'sign',
    path: '/sign',
    name: 'Sign PDF',
    description: 'Draw your signature, or sign with a digital certificate.',
    category: 'security',
    tone: 'rose',
    icon: ['M3.5 20.5h17', 'M3.5 16.3c3.6 0 3.3-12.8 6.8-12.8 3.5 0 1.4 10 4.8 10 2.4 0 2.2-3.7 5.4-3.7'],
    web: false,
  },
  {
    id: 'protect',
    path: '/protect',
    name: 'Protect PDF',
    description: 'Lock a file with a password before you share it.',
    category: 'security',
    tone: 'rose',
    icon: ['M12 3.5l8.3 2.9v5.8c0 4.6-3.5 7.4-8.3 8.3-4.8-.9-8.3-3.7-8.3-8.3V6.4Z', 'M8.9 12.1 11 14.2l4.1-4.2'],
    web: false,
  },
  {
    id: 'pdf-to-image',
    path: '/pdf-to-image',
    name: 'PDF to image',
    description: 'Save pages as JPG or PNG images at the resolution you need.',
    category: 'convert',
    tone: 'amber',
    icon: [
      'M20.5 12.5V6.1a2.6 2.6 0 0 0-2.6-2.6H6.1A2.6 2.6 0 0 0 3.5 6.1v7.8a2.6 2.6 0 0 0 2.6 2.6H10',
      'M7.6 8.6a1.6 1.6 0 1 0 3.2 0 1.6 1.6 0 1 0-3.2 0Z',
      'M17 14v6.5M14 17.5l3 3 3-3',
    ],
    web: false,
  },
];

/** The web viewer, where unavailable tools point people instead. */
export const VIEWER_PATH = '/view';
