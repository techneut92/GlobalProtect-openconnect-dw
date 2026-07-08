// Disable user zoom in the webview. The window is a fixed-size, non-resizable
// chrome-less panel, so zooming only ever corrupts the layout — there's no
// reason to allow it. Covers every zoom vector on WebKitGTK:
//   • Ctrl + mouse wheel  (touchpad pinch is delivered as ctrl+wheel here too)
//   • Ctrl + '+' / '-' / '=' / '0'
//   • Safari-style gesture events (harmless no-op elsewhere)
(function () {
  const stop = (e) => {
    e.preventDefault();
    e.stopPropagation();
  };
  window.addEventListener(
    'wheel',
    (e) => {
      if (e.ctrlKey) stop(e);
    },
    { passive: false, capture: true }
  );
  window.addEventListener(
    'keydown',
    (e) => {
      if ((e.ctrlKey || e.metaKey) && ['+', '-', '=', '0'].includes(e.key)) stop(e);
    },
    { passive: false, capture: true }
  );
  ['gesturestart', 'gesturechange', 'gestureend'].forEach((evt) =>
    window.addEventListener(evt, stop, { passive: false, capture: true })
  );
})();
