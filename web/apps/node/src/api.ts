export interface PublicNodeStatus {
  role: 'public-replica';
  networkId: string;
  finalityMode: string;
  localFinalizedHeight: string;
  upstreamFinalizedHeight: string | null;
  synchronized: boolean;
  activeUpstream: string | null;
  lastError: string | null;
}

export async function loadNodeStatus(): Promise<PublicNodeStatus> {
  const response = await fetch('/api/status', {
    headers: { accept: 'application/json' },
    cache: 'no-store',
  });
  if (!response.ok) {
    throw new Error(`Public node returned HTTP ${response.status}.`);
  }
  return (await response.json()) as PublicNodeStatus;
}
