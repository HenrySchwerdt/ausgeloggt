import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

export const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-base font-medium transition-colors select-none disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-3.5 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-primary text-primary-fg hover:bg-primary/90",
        secondary: "bg-muted text-fg hover:bg-muted-hover",
        ghost: "text-fg-muted hover:bg-muted hover:text-fg",
        outline: "border border-border-strong text-fg hover:bg-muted",
        destructive: "text-s-red hover:bg-s-red/10",
      },
      size: {
        default: "h-[30px] px-3",
        sm: "h-6 px-2 text-sm",
        icon: "size-[30px] [&_svg]:size-4",
        "icon-sm": "size-6 [&_svg]:size-3.5",
      },
    },
    defaultVariants: { variant: "secondary", size: "default" },
  },
);

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(({ className, variant, size, type = "button", ...props }, ref) => (
  <button ref={ref} type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />
));
Button.displayName = "Button";
