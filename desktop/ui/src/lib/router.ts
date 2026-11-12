import { useSyncExternalStore } from "react";

export type Route =
  | { page: "dashboard" }
  | { page: "accounts"; id?: string }
  | { page: "newsletter" }
  | { page: "deletions" }
  | { page: "sources" }
  | { page: "settings" };

export type Page = Route["page"];

function parse(hash: string): Route {
  const parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean);
  switch (parts[0]) {
    case "accounts": return { page: "accounts", id: parts[1] };
    case "newsletter": return { page: "newsletter" };
    case "deletions": return { page: "deletions" };
    case "sources": return { page: "sources" };
    case "settings": return { page: "settings" };
    default: return { page: "dashboard" };
  }
}

export function href(r: Route): string {
  return r.page === "dashboard" ? "#/" : r.page === "accounts" && r.id ? `#/accounts/${r.id}` : `#/${r.page}`;
}

export function navigate(r: Route) {
  const h = href(r);
  if (location.hash !== h) location.hash = h;
}

let cached = { hash: "", route: parse("") };
function snapshot(): Route {
  if (cached.hash !== location.hash) cached = { hash: location.hash, route: parse(location.hash) };
  return cached.route;
}
function subscribe(cb: () => void) {
  window.addEventListener("hashchange", cb);
  return () => window.removeEventListener("hashchange", cb);
}

export function useRoute(): Route {
  return useSyncExternalStore(subscribe, snapshot);
}
