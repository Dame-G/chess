// Before the first paint: the saved choice (chess.scheme: "light", "dark", or absent for System) decides data-scheme.
// A file rather than an inline script, so the Content-Security-Policy can forbid inline scripts.
(function () {
  try {
    var c = JSON.parse(localStorage.getItem("chess.scheme"));
    var d = c === "dark" || (c !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
    var r = document.documentElement;
    r.dataset.scheme = d ? "dark" : "light";
    r.style.colorScheme = d ? "dark" : "light";
  } catch {}
})();
