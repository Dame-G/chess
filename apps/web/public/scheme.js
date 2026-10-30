// Before the first paint: the saved choice (chess.scheme: "light", "dark", or absent for System) decides data-scheme.
// A file rather than an inline script, so the Content-Security-Policy can forbid inline scripts.
(function () {
  try {
    // The app was called Dameless: what this browser remembered under its old prefix is carried over once.
    var old = [];
    for (var i = 0; i < localStorage.length; i++) {
      var k = localStorage.key(i);
      if (k && k.indexOf("dameless.") === 0) old.push(k);
    }
    old.forEach(function (k) {
      var n = "chess." + k.slice(9);
      if (localStorage.getItem(n) === null) localStorage.setItem(n, localStorage.getItem(k));
      localStorage.removeItem(k);
    });
  } catch {}
  try {
    var c = JSON.parse(localStorage.getItem("chess.scheme"));
    var d = c === "dark" || (c !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
    var r = document.documentElement;
    r.dataset.scheme = d ? "dark" : "light";
    r.style.colorScheme = d ? "dark" : "light";
  } catch {}
})();
