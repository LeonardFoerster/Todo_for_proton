/**
 * Popover-API (oberste Ebene, auch über modalen Dialogen – P6-02). Nur setzen, wo die Engine sie kann; jsdom
 * versteckt `[popover]` sonst, ohne es zeigen zu können. Ohne API bleibt es bei `position: fixed`.
 */
export const POPOVER: "manual" | undefined =
  typeof HTMLElement !== "undefined" && "showPopover" in HTMLElement.prototype ? "manual" : undefined;

/** In die oberste Ebene holen, falls möglich. */
export function showOnTop(el: HTMLElement) {
  if (!POPOVER) return;
  try {
    if (!el.matches(":popover-open")) el.showPopover();
  } catch {
    // z. B. schon entfernt – dann genügt die feste Position
  }
}
