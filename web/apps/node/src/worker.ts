const ORIGIN = 'https://r1.verita.tech';
const ORIGIN_PREFIX = '/payrail/node';

interface WorkerEnvironment {
  ASSETS: { fetch(request: Request): Promise<Response> };
}

const corsHeaders = {
  'access-control-allow-headers': 'content-type',
  'access-control-allow-methods': 'GET, HEAD, POST, OPTIONS',
  'access-control-allow-origin': '*',
  'access-control-max-age': '86400',
};

const siteHeaders = {
  'content-security-policy':
    "default-src 'self'; base-uri 'none'; connect-src 'self'; form-action 'none'; frame-ancestors 'none'; img-src 'self' data:; object-src 'none'; script-src 'self'; style-src 'self'",
  'permissions-policy':
    'camera=(), geolocation=(), microphone=(), payment=(), usb=()',
  'referrer-policy': 'strict-origin-when-cross-origin',
  'x-content-type-options': 'nosniff',
  'x-frame-options': 'DENY',
};

function isPublicApiPath(pathname: string): boolean {
  return (
    pathname === '/api/status' ||
    pathname === '/api/explorer' ||
    pathname === '/api/transactions' ||
    pathname.startsWith('/api/accounts/') ||
    pathname.startsWith('/api/contracts/') ||
    pathname === '/health/live' ||
    pathname === '/health/ready'
  );
}

function withPublicHeaders(response: Response): Response {
  const headers = new Headers(response.headers);
  headers.set('cache-control', 'no-store');
  headers.set('x-content-type-options', 'nosniff');
  for (const [name, value] of Object.entries(corsHeaders)) {
    headers.set(name, value);
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

function withSiteHeaders(response: Response): Response {
  const headers = new Headers(response.headers);
  for (const [name, value] of Object.entries(siteHeaders)) {
    headers.set(name, value);
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

export default {
  async fetch(
    request: Request,
    environment: WorkerEnvironment,
  ): Promise<Response> {
    const incoming = new URL(request.url);
    if (!isPublicApiPath(incoming.pathname)) {
      return withSiteHeaders(await environment.ASSETS.fetch(request));
    }
    if (request.method === 'OPTIONS') {
      return new Response(null, { status: 204, headers: corsHeaders });
    }
    if (!['GET', 'HEAD', 'POST'].includes(request.method)) {
      return withPublicHeaders(
        new Response('method not allowed', { status: 405 }),
      );
    }

    const upstream = new URL(ORIGIN);
    upstream.pathname = `${ORIGIN_PREFIX}${incoming.pathname}`;
    upstream.search = incoming.search;
    const headers = new Headers(request.headers);
    headers.delete('authorization');
    headers.delete('cookie');
    headers.set('host', upstream.host);
    const init: RequestInit = {
      method: request.method,
      headers,
      redirect: 'manual',
    };
    if (request.method === 'POST') {
      init.body = request.body;
    }
    return withPublicHeaders(await fetch(new Request(upstream, init)));
  },
};
