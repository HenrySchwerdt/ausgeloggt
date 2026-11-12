import { Toaster as Sonner } from "sonner";
import { useStore } from "@/lib/store";

export function Toaster() {
  const theme = useStore((s) => s.theme);
  return (
    <Sonner
      theme={theme}
      position="bottom-right"
      gap={8}
      offset={20}
      visibleToasts={3}
      icons={{ error: <span className="size-1.5 rounded-full bg-s-red" />, success: <span className="size-1.5 rounded-full bg-s-green" /> }}
      toastOptions={{
        style: {
          background: "var(--surface)", color: "var(--fg)", border: "1px solid var(--border)", borderRadius: "8px",
          boxShadow: "var(--popover-shadow)", fontSize: "13px", padding: "10px 14px", gap: "10px",
        },
      }}
    />
  );
}
