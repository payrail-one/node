import { html } from 'workstar';

export type WalletIcon =
  | 'identity'
  | 'live'
  | 'notification'
  | 'security'
  | 'recovery'
  | 'send'
  | 'request'
  | 'explorer'
  | 'link'
  | 'qr'
  | 'sms';

export function walletIcon(icon: WalletIcon) {
  switch (icon) {
    case 'identity':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="3" y="5" width="18" height="14" rx="3"></rect>
        <circle cx="8" cy="11" r="2"></circle>
        <path
          d="M5.5 16c.7-1.5 1.5-2.2 2.5-2.2s1.8.7 2.5 2.2M13 10h5M13 14h4"
        ></path>
      </svg>`;
    case 'live':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="2.2"></circle>
        <path
          d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.6 4.6a10.5 10.5 0 0 0 0 14.8M19.4 4.6a10.5 10.5 0 0 1 0 14.8"
        ></path>
      </svg>`;
    case 'notification':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4"
        ></path>
      </svg>`;
    case 'security':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 3 20 6v5c0 5-3.3 8.3-8 10-4.7-1.7-8-5-8-10V6l8-3Z"></path>
        <path d="m8.5 12 2.2 2.2 4.8-5"></path>
      </svg>`;
    case 'recovery':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="3" y="4" width="11" height="14" rx="2"></rect>
        <rect x="16" y="8" width="5" height="12" rx="1.5"></rect>
        <path d="m10 9 3 3-3 3M7 12h6"></path>
      </svg>`;
    case 'send':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M5 19 19 5M10 5h9v9"></path>
      </svg>`;
    case 'request':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 4v16M5 13l7 7 7-7"></path>
      </svg>`;
    case 'explorer':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="8"></circle>
        <circle cx="12" cy="12" r="3"></circle>
        <path d="M12 2v2M22 12h-2M12 22v-2M2 12h2"></path>
      </svg>`;
    case 'link':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="m9.5 14.5 5-5M7 17l-1 1a3.5 3.5 0 0 1-5-5l4-4a3.5 3.5 0 0 1 5 0M17 7l1-1a3.5 3.5 0 0 1 5 5l-4 4a3.5 3.5 0 0 1-5 0"
        ></path>
      </svg>`;
    case 'qr':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM15 14h2v2h-2zM19 14h1v3h-3v3h-3v-2M19 20h1"
        ></path>
      </svg>`;
    case 'sms':
      return html`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 5h16v12H9l-5 4V5ZM8 9h8M8 13h5"></path>
      </svg>`;
  }
}
