// ausgeloggt site: vanilla ES module, no dependencies, no network.
const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];

/* header border on scroll */
const hdr = $(".hdr");
const onScroll = () => hdr.classList.toggle("scrolled", scrollY > 8);
addEventListener("scroll", onScroll, { passive: true });
onScroll();

/* reveal on scroll */
const still = new URLSearchParams(location.search).has("static"); // ?static shows everything at once (screenshots, print)
if (!reduce && !still && "IntersectionObserver" in window) {
  const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); } }), { rootMargin: "0px 0px -8% 0px" });
  $$(".rv").forEach((el) => io.observe(el));
} else $$(".rv").forEach((el) => el.classList.add("in"));

/* ---------- hero ledger: log accounts out one by one while the counter ticks down ---------- */
const ledger = $("#ledger");
if (ledger) {
  const list = $(".rows", ledger);
  const countEl = $("#count", ledger);
  const stateEl = $("#state", ledger);
  const pool = JSON.parse($("#ledger-data").textContent);   // [{n, d, t}]
  const labels = JSON.parse(ledger.dataset.labels);          // tags: delete / unsub / cancel
  const start = 73;
  let left = start, next = 0, timer = 0, running = false;

  const make = (i, extra = "") => {
    const p = pool[i % pool.length];
    const li = document.createElement("li");
    li.className = "row " + extra;
    li.innerHTML = `<span class="av" aria-hidden="true"></span><span class="nm"><b></b><span></span></span><span class="tag"></span>`;
    li.children[0].textContent = p.n[0];
    li.querySelector("b").textContent = p.n;
    li.querySelector(".nm span").textContent = p.d;
    li.querySelector(".tag").textContent = labels[p.t];
    return li;
  };
  const render = () => { countEl.textContent = left; };

  // initial rows
  for (let i = 0; i < 8; i++) list.append(make(next++));
  render();

  if (reduce) {
    // static, calm snapshot: a few rows already logged out
    $$(".row", list).slice(0, 3).forEach((r) => r.classList.add("out"));
    left = start - 3; render();
  } else {
    let delay = 1100;
    const step = () => {
      if (!running) return;
      const rows = $$(".row:not(.out):not(.gone)", list);
      const row = rows[0];
      if (row) {
        row.classList.add("out");
        left = Math.max(0, left - 1);
        render();
        setTimeout(() => {
          row.classList.add("gone");
          setTimeout(() => row.remove(), 700);
          if (left - rows.length + 1 > 0 && $$(".row", list).length < 12) list.append(make(next++, "enter"));
        }, 420);
      }
      if (left <= 0) {
        ledger.classList.add("done");
        stateEl.textContent = stateEl.dataset.done;
        running = false;
        return;
      }
      delay = Math.max(240, delay * 0.93);
      timer = setTimeout(step, delay);
    };
    const go = () => { if (!running && left > 0) { running = true; timer = setTimeout(step, 900); } };
    const stop = () => { running = false; clearTimeout(timer); };
    new IntersectionObserver((es) => es.forEach((e) => (e.isIntersecting && !document.hidden ? go() : stop())), { threshold: 0.2 }).observe(ledger);
    document.addEventListener("visibilitychange", () => (document.hidden ? stop() : ledger.getBoundingClientRect().top < innerHeight && go()));
  }
}

/* ---------- scroll-driven screenshot switcher ---------- */
const steps = $$(".step[data-shot]");
const pics = $$(".stage-win picture");
if (steps.length && pics.length && "IntersectionObserver" in window) {
  const set = (id) => {
    steps.forEach((s) => s.classList.toggle("on", s.dataset.shot === id));
    pics.forEach((p) => p.classList.toggle("on", p.dataset.shot === id));
  };
  set(steps[0].dataset.shot);
  const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) set(e.target.dataset.shot); }), { rootMargin: "-45% 0px -45% 0px" });
  steps.forEach((s) => io.observe(s));
}
