import * as React from "react";
import { cn } from "@/lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(({ className, type = "text", ...props }, ref) => (
  <input
    ref={ref}
    type={type}
    className={cn(
      "h-[30px] w-full min-w-0 rounded-md border border-border-strong bg-transparent px-2.5 text-base text-fg outline-none transition-colors placeholder:text-fg-faint hover:border-fg-faint/60 focus-visible:border-fg-muted focus-visible:outline-none disabled:opacity-50",
      className,
    )}
    {...props}
  />
));
Input.displayName = "Input";

export const Textarea = React.forwardRef<HTMLTextAreaElement, React.TextareaHTMLAttributes<HTMLTextAreaElement>>(({ className, ...props }, ref) => (
  <textarea
    ref={ref}
    className={cn(
      "min-h-[72px] w-full resize-none rounded-md border border-border-strong bg-transparent px-2.5 py-1.5 text-base text-fg outline-none transition-colors placeholder:text-fg-faint hover:border-fg-faint/60 focus-visible:border-fg-muted focus-visible:outline-none",
      className,
    )}
    {...props}
  />
));
Textarea.displayName = "Textarea";
