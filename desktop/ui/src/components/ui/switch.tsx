import { Switch as S } from "radix-ui";
import { cn } from "@/lib/utils";

export function Switch({ className, ...p }: React.ComponentProps<typeof S.Root>) {
  return (
    <S.Root
      className={cn("relative inline-flex h-[18px] w-[30px] shrink-0 items-center rounded-full bg-border-strong transition-colors data-[state=checked]:bg-primary disabled:opacity-50", className)}
      {...p}
    >
      <S.Thumb className="block size-3.5 translate-x-[2px] rounded-full bg-bg shadow-sm transition-transform data-[state=checked]:translate-x-[14px]" />
    </S.Root>
  );
}
