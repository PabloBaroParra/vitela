// Where the viewer tells the workspace which page the reader is on (zero-
// based), so a control in the workspace bar — Export images' "Current page"
// — can ask for it without owning the viewer's scroll. Nothing listens by
// default.

import { createContext } from 'react';

export const ReportCurrentPageContext = createContext<(pageIndex: number) => void>(() => {});
