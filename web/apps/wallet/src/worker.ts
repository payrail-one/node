const ORIGIN = 'https://r1.verita.tech';

export default {
  async fetch(request: Request): Promise<Response> {
    const incoming = new URL(request.url);
    const upstream = new URL(ORIGIN);
    upstream.search = incoming.search;
    const isAppleAssociation =
      incoming.pathname === '/.well-known/apple-app-site-association';
    const isAsset = /\.[a-z0-9]+$/i.test(incoming.pathname);
    upstream.pathname = incoming.pathname.startsWith('/api/')
      ? `/payrail${incoming.pathname}`
      : isAsset || isAppleAssociation
        ? `/payrail/wallet${incoming.pathname}`
        : '/payrail/wallet/index.html';
    const headers = new Headers(request.headers);
    headers.set('host', upstream.host);
    const response = await fetch(
      new Request(upstream, {
        method: request.method,
        headers,
        body: request.body,
        redirect: 'follow',
      }),
    );
    if (!isAppleAssociation) return response;
    const body = await response.text();
    if (!response.ok || !isAppleAssociationDocument(body)) {
      return new Response('Apple association is not configured.', {
        status: 404,
        headers: {
          'cache-control': 'no-store',
          'content-type': 'text/plain; charset=utf-8',
        },
      });
    }
    const responseHeaders = new Headers(response.headers);
    responseHeaders.set('content-type', 'application/json');
    responseHeaders.set('cache-control', 'public, max-age=3600');
    return new Response(body, {
      status: response.status,
      statusText: response.statusText,
      headers: responseHeaders,
    });
  },
};

function isAppleAssociationDocument(body: string): boolean {
  try {
    const value: unknown = JSON.parse(body);
    if (!isRecord(value) || !isRecord(value.applinks)) return false;
    const details = value.applinks.details;
    return (
      Array.isArray(details) &&
      details.length > 0 &&
      details.every(
        (detail) =>
          isRecord(detail) &&
          typeof detail.appID === 'string' &&
          detail.appID.length > 0 &&
          Array.isArray(detail.paths),
      )
    );
  } catch {
    return false;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
