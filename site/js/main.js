// ausgeloggt site: no dependencies, no network.
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;

const hdr = document.querySelector(".hdr");
const onScroll = () => hdr.classList.toggle("scrolled", scrollY > 8);
addEventListener("scroll", onScroll, { passive: true });
onScroll();

// Hero: the app reports one finished action after the other.
const toast = document.querySelector("#toast");
if (toast && !reduce) {
  const items = JSON.parse(toast.dataset.items); // [{g, t}]
  const use = toast.querySelector("use");
  const text = toast.querySelector("span");
  let i = 0;
  setInterval(() => {
    toast.classList.add("out");
    setTimeout(() => {
      i = (i + 1) % items.length;
      use.setAttribute("href", "#g-" + items[i].g);
      text.textContent = items[i].t;
      toast.classList.remove("out");
    }, 380);
  }, 3600);
}
