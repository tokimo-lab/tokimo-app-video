import { createContext, useContext } from "react";

interface ActiveLibraryInfo {
  id: string | null;
  type: string | null;
}

const ActiveLibraryContext = createContext<ActiveLibraryInfo>({
  id: null,
  type: null,
});

export const ActiveLibraryProvider = ActiveLibraryContext.Provider;

export function useActiveLibrary(): ActiveLibraryInfo {
  return useContext(ActiveLibraryContext);
}
