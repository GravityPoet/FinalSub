import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "./Button";

export function ActionMenu({ label, children, items }: {
  label: string;
  children?: ReactNode;
  items: Array<{ label: string; action: () => void }>;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = () => { setOpen(false); root.current?.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')?.focus(); };
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus());
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", outside);
    return () => { cancelAnimationFrame(frame); document.removeEventListener("pointerdown", outside); };
  }, [open]);
  return <div ref={root} className="relative shrink-0" onKeyDown={(event) => {
    if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); close(); }
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      if (!open) { setOpen(true); return; }
      const buttons = Array.from(root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? []);
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (current + (event.key === "ArrowUp" ? -1 : 1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    }
    if (event.key === "Tab") setOpen(false);
  }}>
    <Button type="button" variant="secondary" size="sm" aria-haspopup="menu" aria-expanded={open} aria-label={label} onClick={() => setOpen(!open)}>{children ?? label}</Button>
    {open && <div role="menu" aria-label={label} className="absolute right-0 top-full z-50 mt-1 w-44 max-w-[calc(100vw-2rem)] rounded-xl border border-border-default bg-surface-raised p-1 shadow-xl">
      {items.map((item) => <button key={item.label} role="menuitem" type="button" className="block w-full rounded-lg px-3 py-2.5 text-left text-xs font-medium text-text-primary hover:bg-surface-overlay focus:bg-surface-overlay" onClick={() => { close(); item.action(); }}>{item.label}</button>)}
    </div>}
  </div>;
}
