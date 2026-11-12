import { useSyncExternalStore } from "react";

export type Route =
  | { page: "dashboard" }
  | { page: "review" }
  | { page: "accounts"; id?: string }
  | { page: "cleanup"; tidy?: boolean }
  | { page: "newsletter" }
  | { page: "billing" }
  | { page: "security" }
  | { page: "requests" }
  | { page: "sources" }
  | { page: "settings"; section?: string };

export type Page = Route["page"];

function parse(hash: string): Route {
  const parts = hash.replace(/^#\/?/, "").split("?")[0]!.split("/").filter(Boolean);
  switch (parts[0]) {
    case "review": return { page: "review" };
    case "accounts": return { page: "accounts", id: parts[1] };
    case "cleanup": return { page: "cleanup", tidy: parts[1] === "tidy" };
    case "newsletter": return { page: "newsletter" };
    case "billing": return { page: "billing" };
    case "security": return { page: "security" };
    case "requests":
    case "deletions": return { page: "requests" };
    case "sources": return { page: "sources" };
    case "settings": return { page: "settings", section: parts[1] };
    default: return { page: "dashboard" };
  }
}

export function href(r: Route): string {
  switch (r.page) {
    case "dashboard": return "#/";
    case "accounts": return r.id ? `#/accounts/${r.id}` : "#/accounts";
    case "cleanup": return r.tidy ? "#/cleanup/tidy" : "#/cleanup";
    case "settings": return r.section ? `#/settings/${r.section}` : "#/settings";
    default: return `#/${r.page}`;
  }
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
