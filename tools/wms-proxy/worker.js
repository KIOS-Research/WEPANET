// A Cloudflare Worker that lets the web app read map services whose servers
// do not allow it (no CORS headers) or only answer over http, which an https
// page may not load. The app asks  https://<worker>/?url=<encoded service URL>
// only after a direct request has failed (crates/eui-ui/src/layers.rs).
//
// It is not an open proxy: it answers only the app's own pages (ALLOWED_ORIGINS),
// only GET, and only WMS or ArcGIS REST map service requests.

const ALLOWED_ORIGINS = [
  "https://kios-research.github.io",
  /^http:\/\/(localhost|127\.0\.0\.1)(:\d+)?$/,
];

// What a map service answers: pictures, capabilities, identify results, errors.
const ALLOWED_TYPES = /^(image\/|text\/|application\/(.*xml|json|geo\+json|vnd\.ogc\.))/i;

// The largest answer passed on (bytes).
const MAX_BYTES = 20 * 1024 * 1024;

function originAllowed(origin) {
  return ALLOWED_ORIGINS.some((o) => (typeof o === "string" ? o === origin : o.test(origin)));
}

function looksLikeMapService(target) {
  const query = target.search.toLowerCase();
  const path = target.pathname.toLowerCase();
  return query.includes("service=wms") || path.includes("/mapserver");
}

function privateHost(host) {
  return (
    host === "localhost" ||
    host.endsWith(".local") ||
    host.endsWith(".internal") ||
    /^(10|127|0)\./.test(host) ||
    /^192\.168\./.test(host) ||
    /^172\.(1[6-9]|2\d|3[01])\./.test(host) ||
    /^169\.254\./.test(host) ||
    host.startsWith("[")
  );
}

function cors(origin) {
  return {
    "Access-Control-Allow-Origin": origin,
    "Access-Control-Allow-Methods": "GET, OPTIONS",
    "Access-Control-Max-Age": "86400",
    Vary: "Origin",
  };
}

function fail(status, message, origin) {
  const headers = { "Content-Type": "text/plain; charset=utf-8" };
  if (origin) Object.assign(headers, cors(origin));
  return new Response(message, { status, headers });
}

export default {
  async fetch(request) {
    const origin = request.headers.get("Origin") || "";
    if (!originAllowed(origin)) {
      return fail(403, "Only the WEPANET web app may use this proxy.");
    }
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: cors(origin) });
    }
    if (request.method !== "GET") {
      return fail(405, "Only GET", origin);
    }

    const raw = new URL(request.url).searchParams.get("url");
    let target;
    try {
      target = new URL(raw);
    } catch {
      return fail(400, "Missing or bad ?url=", origin);
    }
    if (target.protocol !== "http:" && target.protocol !== "https:") {
      return fail(400, "Only http and https", origin);
    }
    if (privateHost(target.hostname)) {
      return fail(403, "Private addresses are not proxied", origin);
    }
    if (!looksLikeMapService(target)) {
      return fail(403, "Only WMS and ArcGIS REST map service requests are proxied", origin);
    }

    let upstream;
    try {
      upstream = await fetch(target.toString(), {
        headers: { "User-Agent": "WEPANET-map-proxy" },
        redirect: "follow",
        // nothing is kept: every answer comes from the service itself
        cf: { cacheTtl: 0 },
      });
    } catch (e) {
      return fail(502, `Could not reach ${target.host}`, origin);
    }

    const type = upstream.headers.get("Content-Type") || "";
    if (type && !ALLOWED_TYPES.test(type)) {
      return fail(415, `Not a map service answer (${type})`, origin);
    }
    const length = Number(upstream.headers.get("Content-Length") || 0);
    if (length > MAX_BYTES) {
      return fail(413, "Answer too large", origin);
    }

    const headers = cors(origin);
    if (type) headers["Content-Type"] = type;
    headers["Cache-Control"] = "no-store";
    return new Response(upstream.body, { status: upstream.status, headers });
  },
};
