import { attr, html } from 'workstar';

export interface PayrailBrandOptions {
  href?: string;
  label?: string;
  suffix?: string;
}

export function payrailBrand(options: PayrailBrandOptions = {}) {
  return html`<a
    class="pr-brand"
    ${attr('href', options.href ?? 'https://payrail.one')}
    ${attr('aria-label', options.label ?? 'Payrail home')}
  >
    <span class="pr-brand__mark" aria-hidden="true"><i></i><i></i></span>
    <span class="pr-brand__word">payrail</span>
    ${options.suffix
      ? html`<span class="pr-brand__suffix">${options.suffix}</span>`
      : null}
  </a>`;
}
