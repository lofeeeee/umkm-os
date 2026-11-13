// Tema diterapkan sebelum paint (anti-FOUC). CSP 'self' — dimuat sebagai file statis.
(() => {
  try {
    var t = localStorage.getItem("umkm-theme");
    if (t === "dark" || t === "light") document.documentElement.dataset.theme = t;
    else if (matchMedia("(prefers-color-scheme: dark)").matches) document.documentElement.dataset.theme = "dark";
  } catch (e) {}
})();
