// Tiny, synchronous: honours ?theme=light|dark (for screenshots / sharing), otherwise prefers-color-scheme in CSS.
(function () {
  var d = document.documentElement;
  d.classList.add("js");
  try {
    var t = new URLSearchParams(location.search).get("theme");
    if (t === "light" || t === "dark") d.setAttribute("data-theme", t);
  } catch (e) {}
})();
