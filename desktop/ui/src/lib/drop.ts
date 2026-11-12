import { create } from "zustand";

/** Files dropped onto the window, consumed by the Quellen page. */
export const useDrop = create<{ pending: string[]; push(p: string[]): void; clear(): void }>((set) => ({
  pending: [],
  push: (p) => set({ pending: p }),
  clear: () => set({ pending: [] }),
}));
