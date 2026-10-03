import { payrailBrand } from '@platform/ui-kit';
import { attr, html, mount, on, signal } from 'workstar';
import '@platform/ui-kit/styles.css';
import { loadNodeStatus, type PublicNodeStatus } from './api';
import './style.css';

const installCommand = `git clone https://github.com/payrail-one/node.git
cd node
docker compose up --build --detach`;

function createApp() {
  const status = signal<PublicNodeStatus | null>(null);
  const reachable = signal(false);
  const checkedAt = signal('Connecting');
  const copied = signal(false);

  const refresh = async () => {
    try {
      status.value = await loadNodeStatus();
      reachable.value = true;
      checkedAt.value = new Date().toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
      });
    } catch {
      reachable.value = false;
      checkedAt.value = 'Unavailable';
    }
  };

  const copyInstall = async () => {
    await navigator.clipboard.writeText(installCommand);
    copied.value = true;
    window.setTimeout(() => {
      copied.value = false;
    }, 1_800);
  };

  void refresh();
  window.setInterval(() => void refresh(), 2_000);

  return html`<div class="shell">
    <header class="pr-public-header">
      ${payrailBrand()}
      <nav>
        <a href="https://wallet.payrail.one">Wallet</a>
        <a href="https://explorer.payrail.one">Explorer</a>
        <a href="https://devnet.payrail.one">Devnet</a>
        <a class="github-link" href="https://github.com/payrail-one/node"
          >GitHub ↗</a
        >
      </nav>
    </header>

    <main>
      <section class="hero">
        <div class="hero-copy">
          <p class="eyebrow">Independent Payrail infrastructure</p>
          <h1>Public node.<br /><em>Your copy of the truth.</em></h1>
          <p class="lede">
            A self-hosted replica that downloads every finalized block,
            re-executes it locally and keeps its own durable ledger.
          </p>
          <div class="hero-actions">
            <a class="primary-action" href="#run">Run your own node</a>
            <a
              class="secondary-action"
              href="https://github.com/payrail-one/node"
              >View source</a
            >
          </div>
        </div>

        <aside
          class="node-orbit"
          ${attr('data-ready', () =>
            String(Boolean(reachable.value && status.value?.synchronized)),
          )}
        >
          <div class="orbit orbit-one"></div>
          <div class="orbit orbit-two"></div>
          <div class="node-core"><i></i><span>PR</span></div>
          <div class="status-float">
            <span
              ><i></i>${() => nodeState(reachable.value, status.value)}</span
            >
            <strong>#${() => status.value?.localFinalizedHeight ?? '—'}</strong>
            <small>local finalized height</small>
          </div>
        </aside>
      </section>

      <section class="status-strip">
        <article>
          <span
            class="indicator"
            ${attr('data-ready', () =>
              String(Boolean(reachable.value && status.value?.synchronized)),
            )}
          ></span>
          <div>
            <strong>${() => nodeState(reachable.value, status.value)}</strong
            ><small>checked ${checkedAt}</small>
          </div>
        </article>
        <article>
          <div>
            <small>Local height</small
            ><strong
              >#${() => status.value?.localFinalizedHeight ?? '—'}</strong
            >
          </div>
        </article>
        <article>
          <div>
            <small>Upstream height</small
            ><strong
              >#${() => status.value?.upstreamFinalizedHeight ?? '—'}</strong
            >
          </div>
        </article>
        <article>
          <div>
            <small>Sync delta</small
            ><strong>${() => syncDelta(status.value)}</strong>
          </div>
        </article>
      </section>

      <section class="verification panel">
        <div class="section-heading">
          <div>
            <p class="eyebrow">Verification pipeline</p>
            <h2>Trust the execution, not the endpoint.</h2>
          </div>
          <p>
            Each transition is checked against the local parent, network
            identity, block hash and resulting state root before the cursor
            advances.
          </p>
        </div>
        <div class="pipeline">
          <div class="pipeline-step">
            <span>01</span><i class="source-icon"></i
            ><strong>Payrail upstream</strong
            ><small>${() => host(status.value?.activeUpstream)}</small>
          </div>
          <b>→</b>
          <div class="pipeline-step active">
            <span>02</span><i>✓</i><strong>Verify + execute</strong
            ><small>deterministic runtime</small>
          </div>
          <b>→</b>
          <div class="pipeline-step">
            <span>03</span><i class="store-icon"></i><strong>Local LMDB</strong
            ><small>durable independent state</small>
          </div>
          <b>→</b>
          <div class="pipeline-step">
            <span>04</span><i>{ }</i><strong>Public API</strong
            ><small>local finalized reads</small>
          </div>
        </div>
      </section>

      <div class="details-grid">
        <section class="panel identity">
          <div class="panel-title">
            <p class="eyebrow">Node identity</p>
            <span>Live configuration</span>
          </div>
          <dl>
            <div>
              <dt>Role</dt>
              <dd>Public replica</dd>
            </div>
            <div>
              <dt>Network</dt>
              <dd><code>${() => shortHash(status.value?.networkId)}</code></dd>
            </div>
            <div>
              <dt>Finality</dt>
              <dd>${() => mode(status.value?.finalityMode)}</dd>
            </div>
            <div>
              <dt>Upstream</dt>
              <dd>${() => host(status.value?.activeUpstream)}</dd>
            </div>
          </dl>
          <p class="boundary">
            Development network · test assets · no validator authority
          </p>
        </section>

        <section class="panel endpoints">
          <div class="panel-title">
            <p class="eyebrow">Public interface</p>
            <span>Same-origin API</span>
          </div>
          ${endpoint('GET', '/api/status', 'Sync and network status')}
          ${endpoint('GET', '/api/explorer', 'Locally verified blocks')}
          ${endpoint('GET', '/api/accounts/{address}', 'Balance and nonce')}
          ${endpoint('GET', '/api/contracts/{id}', 'Finalized code and state')}
          ${endpoint('POST', '/api/transactions', 'Relay signed envelope')}
          ${endpoint('GET', '/health/ready', 'Synchronization readiness')}
        </section>
      </div>

      <section class="run panel" id="run">
        <div class="run-copy">
          <p class="eyebrow">Open infrastructure</p>
          <h2>Run the same node anywhere.</h2>
          <p>
            No private repository, operator allowlist or signing key is
            required. The API binds to loopback by default and ledger data
            survives container restarts.
          </p>
          <a
            href="https://github.com/payrail-one/node/blob/main/docs/PUBLIC_NODE.md"
            >Read the operator guide →</a
          >
        </div>
        <div class="terminal">
          <div class="terminal-bar">
            <span><i></i><i></i><i></i></span
            ><button type="button" ${on('click', copyInstall)}>
              ${() => (copied.value ? 'Copied' : 'Copy')}
            </button>
          </div>
          <pre><code><span>$</span> git clone https://github.com/payrail-one/node.git
<span>$</span> cd node
<span>$</span> docker compose up --build --detach</code></pre>
        </div>
      </section>
    </main>

    <footer>
      <span>Payrail public node</span>
      <p>Independent state. Verifiable finality. Open operation.</p>
      <a href="https://payrail.one">payrail.one ↗</a>
    </footer>
  </div>`;
}

function endpoint(method: string, path: string, description: string) {
  return html`<div class="endpoint">
    <b>${method}</b><code>${path}</code><span>${description}</span>
  </div>`;
}

function nodeState(
  reachable: boolean,
  status: PublicNodeStatus | null,
): string {
  if (!reachable) return 'Node unreachable';
  return status?.synchronized ? 'Fully synchronized' : 'Synchronizing';
}

function syncDelta(status: PublicNodeStatus | null): string {
  if (!status?.upstreamFinalizedHeight) return '—';
  const local = BigInt(status.localFinalizedHeight);
  const upstream = BigInt(status.upstreamFinalizedHeight);
  return (upstream > local ? upstream - local : 0n).toString();
}

function shortHash(value?: string): string {
  return value ? `${value.slice(0, 10)}…${value.slice(-8)}` : '—';
}

function mode(value?: string): string {
  return value === 'single-node-devnet' ? 'Single-node devnet' : (value ?? '—');
}

function host(value?: string | null): string {
  if (!value) return '—';
  try {
    return new URL(value).host;
  } catch {
    return value;
  }
}

const application = document.querySelector('#app');
if (!application) throw new Error('Missing application mount point.');
mount(application, createApp());
