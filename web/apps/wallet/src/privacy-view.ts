import { payrailBrand } from '@platform/ui-kit';
import { html } from 'workstar';

export function privacyScreen() {
  return html`<div class="shell privacy-shell">
    <header class="pr-public-header">
      ${payrailBrand()}
      <nav>
        <a href="/">Wallet</a>
        <a href="https://explorer.payrail.one">Explorer</a>
      </nav>
    </header>
    <main class="privacy-page">
      <p class="eyebrow">Payrail · Privacy</p>
      <h1>Privacy, without the fine-print maze.</h1>
      <p class="privacy-lead">
        Payrail Wallet is a self-custody development-network wallet. Your
        password and private seed are not sent to Payrail.
      </p>

      <section>
        <h2>Information kept on your device</h2>
        <p>
          The web wallet stores an encrypted wallet vault, a short-lived local
          session, preferences and a random Payrail Code device identifier in
          browser storage. The iOS app stores the active seed in the iOS
          Keychain and may save an encrypted backup only when you export one.
        </p>
      </section>

      <section>
        <h2>Information sent to the network</h2>
        <p>
          To show balances and finality, Payrail processes public wallet
          addresses, checkout identifiers and network requests. When you approve
          an operation, the wallet sends the signed operation—not your private
          key or password—to the Payrail development network.
        </p>
      </section>

      <section>
        <h2>Public ledger data</h2>
        <p>
          Finalized addresses, amounts and transaction data are public testnet
          records. Ledger records are designed to remain available and cannot be
          deleted like an ordinary account record. TEST assets have no monetary
          value.
        </p>
      </section>

      <section>
        <h2>Camera, tracking and third parties</h2>
        <p>
          Camera access is used only when you choose to scan a payment QR code.
          Payrail Wallet does not include advertising SDKs, cross-app tracking
          or analytics. Hosting and network providers may process ordinary
          connection data, such as IP addresses and request timestamps, to
          operate and secure the service.
        </p>
      </section>

      <section>
        <h2>Your choices</h2>
        <p>
          You can lock the wallet, clear the browser's site data, delete the iOS
          wallet from Settings, or export an encrypted backup. Removing local
          data does not remove public ledger records.
        </p>
      </section>

      <section>
        <h2>Contact and changes</h2>
        <p>
          Questions can be sent to
          <a href="mailto:hello@payrail.one">hello@payrail.one</a>. Material
          changes will be reflected on this page with a new effective date.
        </p>
      </section>

      <p class="privacy-date">Effective: 4 October 2026</p>
    </main>
  </div>`;
}
