// Keeps pointer events Rust has no use for on the page's side (bridge.rs).
// Each event the native interpreter passes on is a synchronous request to Rust,
// about 5 to 10 ms on a phone. Its listener for a kind of event sits on the root
// and stays once any element has listened, so after the viewer has been open a
// finger moving anywhere in the app paid for it.
(() => {
  const dx = window.interpreter;
  if (!dx || typeof dx.handleEvent !== "function" || dx.__sbBridge) return;
  dx.__sbBridge = true;
  // Pointers down anywhere, for pinches.
  const down = new Set();
  const forget = (e) => down.delete(e.pointerId);
  addEventListener("pointerdown", (e) => down.add(e.pointerId), { capture: true, passive: true });
  addEventListener("pointerup", forget, { capture: true, passive: true });
  addEventListener("pointercancel", forget, { capture: true, passive: true });
  // `data-pointer` marks what listens to pointers in Rust: "all" for every
  // event, "zoom" for moves only while zoomed or with two fingers down.
  const wanted = (event, name) => {
    const target = event.target instanceof Element ? event.target : null;
    const owner = target && target.closest("[data-pointer]");
    switch (name) {
      case "pointermove":
        if (!owner) return false;
        return owner.dataset.pointer === "all" || down.size > 1 || owner.classList.contains("zoomed");
      case "pointerdown":
      case "pointerup":
      case "pointercancel":
        return !!owner;
      case "pointerenter":
      case "pointerleave":
        return event.pointerType === "mouse" || !!owner;
      default:
        return true;
    }
  };
  const pass = dx.handleEvent.bind(dx);
  dx.handleEvent = (event, name, bubbles) => {
    if (wanted(event, name)) pass(event, name, bubbles);
  };
})();
