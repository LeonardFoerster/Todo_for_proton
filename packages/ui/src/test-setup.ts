// jsdom ergänzen: matchMedia, ResizeObserver (für bind:clientHeight) und Layoutmaße der VirtualList.
if (typeof window !== "undefined") {
  window.matchMedia ??= ((query: string) => ({
    matches: false,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;

  globalThis.ResizeObserver ??= class {
    constructor(private readonly cb: ResizeObserverCallback) {}
    observe(target: Element) {
      this.cb([{ target } as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    unobserve() {}
    disconnect() {}
  };

  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 720 });
}
