(function(){let e=document.createElement(`link`).relList;if(e&&e.supports&&e.supports(`modulepreload`))return;for(let e of document.querySelectorAll(`link[rel="modulepreload"]`))n(e);new MutationObserver(e=>{for(let t of e)if(t.type===`childList`)for(let e of t.addedNodes)e.tagName===`LINK`&&e.rel===`modulepreload`&&n(e)}).observe(document,{childList:!0,subtree:!0});function t(e){let t={};return e.integrity&&(t.integrity=e.integrity),e.referrerPolicy&&(t.referrerPolicy=e.referrerPolicy),e.crossOrigin===`use-credentials`?t.credentials=`include`:e.crossOrigin===`anonymous`?t.credentials=`omit`:t.credentials=`same-origin`,t}function n(e){if(e.ep)return;e.ep=!0;let n=t(e);fetch(e.href,n)}})();var e=class extends Error{status;constructor(e,t){super(e),this.status=t,this.name=`PlatformApiError`}},t=class{#e;#t;constructor(e,t=globalThis.fetch.bind(globalThis)){this.#e=e.replace(/\/$/,``),this.#t=t}status(){return this.#n(`/status`)}account(e){return this.#n(`/accounts/${encodeURIComponent(e)}`)}bridgeAccount(e){return this.#n(`/bridge/accounts/${encodeURIComponent(e)}`)}contract(e){return this.#n(`/contracts/${encodeURIComponent(e)}`)}overview(){return this.#n(`/explorer`)}faucet(e){return this.#n(`/faucet`,{method:`POST`,headers:{"content-type":`application/json`},body:JSON.stringify({address:e})})}submit(e){return this.#n(`/transactions`,{method:`POST`,headers:{"content-type":`application/json`},body:JSON.stringify({envelope:n(e)})})}createCheckout(e){return this.#n(`/checkouts`,{method:`POST`,headers:{"content-type":`application/json`},body:JSON.stringify(e)})}checkout(e){return this.#n(`/checkouts/${encodeURIComponent(e)}`)}issueApprovalCode(e){return this.#n(`/approval-codes`,{method:`POST`,headers:{"content-type":`application/json`},body:JSON.stringify(e)})}approvalCodeChallenge(e){return this.#n(`/approval-codes/sessions/${encodeURIComponent(e)}`)}submitCheckout(e,t){return this.#n(`/checkouts/${encodeURIComponent(e)}/transactions`,{method:`POST`,headers:{"content-type":`application/json`},body:JSON.stringify({envelope:n(t)})})}async#n(t,n){let r=await this.#t(`${this.#e}${t}`,n);if(!r.ok){let t=await r.json().catch(()=>null);throw new e((typeof t?.error==`string`?t.error:r.statusText)||`Request failed.`,r.status)}return await r.json()}};function n(e){return Array.from(e,e=>e.toString(16).padStart(2,`0`)).join(``)}function r(e,t){if(!/^[0-9a-f]+$/.test(e)||e.length%2!=0)throw Error(`Expected canonical lowercase hexadecimal data.`);let n=Uint8Array.from(e.match(/.{2}/g)??[],e=>Number.parseInt(e,16));if(t!==void 0&&n.length!==t)throw Error(`Expected ${t} bytes.`);return n}var i=/^(0|[1-9][0-9]*)(?:\.([0-9]+))?$/;function a(e,t){s(t);let n=e.trim(),r=i.exec(n);if(!r)throw Error(`Enter a positive decimal amount.`);let a=r[2]??``;if(a.length>t)throw Error(`This asset supports at most ${t} decimal places.`);let o=10n**BigInt(t),c=a.padEnd(t,`0`),l=BigInt(r[1])*o+BigInt(c||`0`);if(l<=0n)throw Error(`Amount must be greater than zero.`);return l}function o(e,t){s(t);let n=typeof e==`bigint`?e:BigInt(e);if(n<0n)throw Error(`A balance cannot be negative.`);if(t===0)return n.toString();let r=10n**BigInt(t),i=n/r,a=(n%r).toString().padStart(t,`0`).replace(/0+$/,``);return a.length===0?i.toString():`${i}.${a}`}function s(e){if(!Number.isInteger(e)||e<0||e>18)throw Error(`Asset decimals must be an integer between 0 and 18.`)}var c=e=>Object.freeze(e());function l(e){return e instanceof Uint8Array||ArrayBuffer.isView(e)&&e.constructor.name===`Uint8Array`&&`BYTES_PER_ELEMENT`in e&&e.BYTES_PER_ELEMENT===1}function u(e){if(!l(e))throw TypeError(`Uint8Array expected`)}function d(e,t){return Array.isArray(t)?t.length===0?!0:e?t.every(e=>typeof e==`string`):t.every(e=>Number.isSafeInteger(e)):!1}function f(e){if(typeof e!=`function`)throw TypeError(`function expected`);return!0}function p(e,t){if(typeof t!=`string`)throw TypeError(`${e}: string expected`);return!0}function m(e,t=`number`){if(typeof e!=`number`)throw TypeError(`${t}: expected number, got ${typeof e}`);if(!Number.isSafeInteger(e))throw RangeError(`${t}: expected safe integer, got ${e}`)}function h(e,t){if(!d(!1,t))throw TypeError(`${e}: array of numbers expected`)}var ee=(()=>{let e=[];for(let t=0;t<40;t++)e.push(2**t);return e})();function g(e,t=e.length){let n=Array(t);for(let r=0;r<t;r++)n[r]=e[r];return n}var _=(()=>{try{let e=new TextDecoder;return e.decode(Uint8Array.of(65,48,43,127))===`A0+`?e:void 0}catch{return}})(),v=8192;function y(e){let t=e.length;if(_!==void 0&&t>=12)return _.decode(e);if(t<=v)return String.fromCharCode.apply(null,e);let n=``;for(let r=0;r<t;r+=v)n+=String.fromCharCode.apply(null,e.subarray(r,r+v));return n}function te(e){if(m(e),e<=0||e>8)throw RangeError(`radix2: bits should be in (0..8]`);let t=ee[e]-1;return{encode:n=>{u(n);let r=n.length,i=new Uint8Array(Math.ceil(r*8/e)),a=0,o=0,s=0;for(let c=0;c<r;)for(c+2<r?(a=a<<24|n[c]<<16|n[c+1]<<8|n[c+2],o+=24,c+=3):(a=(a<<8|n[c])&65535,o+=8,c++);o-=e,i[s++]=a>>o&t,!(o<e););return o>0&&(i[s]=a<<e-o&t),i},decode:t=>{let n=t.length,r=new Uint8Array(Math.floor(n*e/8)),i=0,a=0,o=0;for(let s=0;s<n;s++)for(i=(i<<e|t[s])&65535,a+=e;a>=8;a-=8)r[o++]=i>>a-8&255;if(i=i<<8-a&255,a>=e)throw Error(`Excess padding`);if(i>0)throw Error(`Non-zero padding: ${i}`);return r}}}function ne(e,t){let n=e.length;if(n>128)throw Error(`alphabet: max 128 letters`);let r=new Uint8Array(n),i=new Int8Array(128).fill(-1);for(let t=0;t<n;t++){let n=e.charCodeAt(t);if(e.codePointAt(t)!==n||n>127)throw Error(`alphabet: single-char ASCII letters only`);r[t]=n,i[n]=t}if(t!==void 0)for(let e of Object.keys(t)){let n=e.charCodeAt(0),r=i[t[e].charCodeAt(0)];if(e.length!==1||n>127||r===void 0||r===-1)throw Error(`alphabet: invalid alias ${e}`);i[n]=r}return{encode:e=>{let t=new Uint8Array(e.length);for(let n=0;n<e.length;n++){let i=e[n],a=r[i];if(a===void 0)throw Error(`alphabet.encode: invalid digit ${i}`);t[n]=a}return y(t)},decode:t=>{p(`decode`,t);let n=t.length,r=new Uint8Array(n);for(let a=0;a<n;a++){let n=t.charCodeAt(a),o=n<128?i[n]:-1;if(o===-1)throw Error(`Unknown letter "${t[a]}". Allowed: ${e}`);r[a]=o}return r}}}function re(e){return f(e),function(...t){try{return e.apply(null,t)}catch{}}}var ie=ne(`qpzry9x8gf2tvdw0s3jn54khce6mua7l`),b=/^[\x21-\x60\x7b-\x7e]+$/;function x(e,t){for(let n=0;n<t.length;n++){let r=t.charCodeAt(n);if(r<33||r>126)throw Error(`${e}: printable ASCII expected`)}}function S(e){let t=e.length,n=new Uint8Array(t);for(let r=0;r<t;r++){let t=e[r];if(t<0||t>=32)throw Error(`alphabet.encode: invalid digit ${t}`);n[r]=t}return n}var C=[996825010,642813549,513874426,1027748829,705979059];function w(e){let t=e>>25,n=(e&33554431)<<5;for(let e=0;e<C.length;e++)(t>>e&1)==1&&(n^=C[e]);return n}function ae(e,t,n=1){let r=e.length,i=1;for(let t=0;t<r;t++){let n=e.charCodeAt(t);if(n<33||n>126)throw Error(`Invalid prefix (${e})`);i=w(i)^n>>5}i=w(i);for(let t=0;t<r;t++)i=w(i)^e.charCodeAt(t)&31;for(let e of t)i=w(i)^e;for(let e=0;e<6;e++)i=w(i);i^=n;let a=new Uint8Array(6);for(let e=0;e<6;e++)a[e]=i>>>5*(5-e)&31;return ie.encode(a)}function oe(e){let t=e===`bech32`?1:734539939,n=te(5),r=e=>{u(e);let t=e.length,n=Array(Math.ceil(t*8/5)),r=0,i=0,a=0;for(let o=0;o<t;o++)for(r=r<<8|e[o],i+=8;i>=5;i-=5)n[a++]=r>>i-5&31;return i>0&&(n[a]=r<<5-i&31),n},i=e=>{h(`radix2.decode`,e);let t=e.length,r=new Uint8Array(t);for(let n=0;n<t;n++){let t=e[n];if(t<0||t>=32)throw Error(`convertRadix2: invalid word=${t}`);r[n]=t}return n.decode(r)},a=re(i);function o(e,n,r=90){p(`bech32.encode prefix`,e),r!==!1&&m(r,`limit`),l(n)&&(n=g(n)),h(`bech32.encode`,n);let i=e.length;if(i===0)throw TypeError(`Invalid prefix length ${i}`);let a=i+7+n.length;if(r!==!1&&a>r)throw TypeError(`Length ${a} exceeds limit ${r}`);x(`bech32.encode prefix`,e);let o=e.toLowerCase(),s=ae(o,n,t);return`${o}1${ie.encode(S(n))}${s}`}function s(e,n=90){p(`bech32.decode input`,e),n!==!1&&m(n,`limit`);let r=e.length;if(r<8||n!==!1&&r>n)throw TypeError(`invalid string length ${r}, expected (8..${n})`);let i=e.toLowerCase();if(e!==i&&!b.test(e))throw x(`bech32.decode input`,e),Error(`mixed-case string not allowed`);let a=i.lastIndexOf(`1`);if(a===0||a===-1)throw Error(`invalid separator "1"`);let o=i.slice(0,a),s=i.slice(a+1);if(s.length<6)throw Error(`invalid data length`);let c=ie.decode(s),l=g(c,c.length-6),u=ae(o,l,t);if(!s.endsWith(u))throw Error(`Invalid checksum in ${e}`);return{prefix:o,words:l}}let c=re(s);function d(e,t=90){let{prefix:n,words:r}=s(e,t);return{prefix:n,words:r,bytes:i(r)}}function f(e,t){return o(e,r(t))}return{encode:o,decode:s,encodeFromBytes:f,decodeToBytes:d,decodeUnsafe:c,fromWords:i,fromWordsUnsafe:a,toWords:r}}var se=c(()=>oe(`bech32m`)),T=16*1024,E=4*1024,ce=100000n;function D(e,t){if(ue(t.idempotencyKey,32,`Idempotency key`),ue(t.salt,32,`Contract salt`),t.code.length===0||t.code.length>16384)throw Error(`Contract code must contain 1–${T} bytes.`);return de(Uint8Array.of(4),r(t.networkId,32),t.idempotencyKey,r(t.assetId,32),e,t.salt,O(BigInt(t.code.length),4),t.code,O(t.fee,16),O(t.nonce,8),O(t.validUntilHeight,8))}function le(e,t){ue(t.idempotencyKey,32,`Idempotency key`);let n=new TextEncoder().encode(t.entrypoint);if(n.length===0||n.length>32||!/^[a-z][a-z0-9_]{0,31}$/.test(t.entrypoint))throw Error(`Contract entrypoint must be a canonical lowercase name.`);let i=Uint8Array.from(t.args??[]);if(i.length>4096)throw Error(`Contract arguments exceed ${E} bytes.`);if(t.executionLimit<=0n||t.executionLimit>100000n)throw Error(`Execution limit must be between 1 and ${ce}.`);return de(Uint8Array.of(5),r(t.networkId,32),t.idempotencyKey,r(t.assetId,32),e,r(t.contractId,32),Uint8Array.of(n.length),n,O(BigInt(i.length),4),i,O(t.attachedAmount??0n,16),O(t.fee,16),O(t.executionLimit,8),O(t.nonce,8),O(t.validUntilHeight,8))}function ue(e,t,n){if(e.length!==t)throw Error(`${n} must be ${t} bytes.`)}function O(e,t){if(e<0n)throw Error(`Unsigned values cannot be negative.`);let n=new Uint8Array(t),r=e;for(let e=t-1;e>=0;--e)n[e]=Number(r&255n),r>>=8n;if(r!==0n)throw Error(`Value does not fit in ${t} bytes.`);return n}function de(...e){let t=new Uint8Array(e.reduce((e,t)=>e+t.length,0)),n=0;for(let r of e)t.set(r,n),n+=r.length;return t}var fe=new TextEncoder().encode(`ledger.authorization\0`),pe=new TextEncoder().encode(`ledger.envelope\0`),me=`payrail.wallet.v1`,he=6e5,ge=0,_e=new TextEncoder().encode(`payrail.approval.issue.v1\0`);function ve(e){if(!Ce(e)||e.version!==1)throw Error(`Unsupported wallet backup.`);let t=k(e,`address`),n=k(e,`publicKey`),r=k(e,`salt`),i=k(e,`iv`),a=k(e,`ciphertext`),o=M(n),s=M(r),c=M(i),l=M(a);if(o.length!==32||s.length!==16||c.length!==12||l.length===0)throw Error(`Wallet backup is malformed.`);return{version:1,address:t,publicKey:n,salt:r,iv:i,ciphertext:a}}function ye(e,t=Date.now()){let n=e.split(`
`);if(n.length!==7||n[0]!==`payrail.phone-link.v1`)throw Error(`Unsupported phone-link request.`);let r=new Map(n.slice(1).map(e=>{let t=e.indexOf(`=`);if(t<=0)throw Error(`Malformed phone-link request.`);return[e.slice(0,t),e.slice(t+1)]})),i=r.get(`challenge`)??``,a=r.get(`nonce`)??``,o=Me(r.get(`expires_at`)),s=je(r.get(`per_entry_limit_minor`)),c=je(r.get(`daily_limit_minor`)),l=Me(r.get(`mandate_expires_at`));if(r.size!==6||!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u.test(i)||!/^[A-Za-z0-9_-]{43}$/u.test(a)||o<=t||o>t+15*6e4||c<s||l<=t||l>t+366*864e5)throw Error(`Phone-link request terms are invalid or expired.`);return{challengeId:i,expiresAt:o,perEntryLimitMinor:s,dailyLimitMinor:c,mandateExpiresAt:l}}async function be(e,t){Oe(t);let n=await crypto.subtle.generateKey(`Ed25519`,!0,[`sign`,`verify`]),r=new Uint8Array(await crypto.subtle.exportKey(`raw`,n.publicKey)),i=new Uint8Array(await crypto.subtle.exportKey(`pkcs8`,n.privateKey)),a=await crypto.subtle.importKey(`pkcs8`,i,`Ed25519`,!1,[`sign`]),o=crypto.getRandomValues(new Uint8Array(16)),s=crypto.getRandomValues(new Uint8Array(12)),c=A(e,r),l=await Ee(t,o),u;try{u=await crypto.subtle.encrypt({name:`AES-GCM`,iv:s,additionalData:De(e,r)},l,i)}finally{i.fill(0)}return{wallet:we(e,a,r),vault:{version:1,address:c,publicKey:j(r),salt:j(o),iv:j(s),ciphertext:j(new Uint8Array(u))}}}async function xe(e,t){if(!Ce(t)||t.version!==1)throw Error(`Wallet session is invalid.`);let{address:n,publicKey:r,privateKey:i,expiresAtMs:a}=t;if(typeof n!=`string`||typeof r!=`string`||!(i instanceof CryptoKey)||typeof a!=`number`||!Number.isSafeInteger(a)||a<=Date.now())throw Error(`Wallet session has expired.`);let o=M(r);if(o.length!==32||A(e,o)!==n||i.extractable||i.algorithm.name!==`Ed25519`||i.type!==`private`||i.usages.length!==1||i.usages[0]!==`sign`)throw Error(`Wallet session is invalid.`);let s=crypto.getRandomValues(new Uint8Array(32)),c=await crypto.subtle.importKey(`raw`,o,`Ed25519`,!1,[`verify`]),l=await crypto.subtle.sign(`Ed25519`,i,s);if(!await crypto.subtle.verify(`Ed25519`,c,l,s))throw Error(`Wallet session does not match this account.`);return we(e,i,o)}async function Se(e,t,n){Oe(t);let r=ve(n),i=M(r.publicKey),a=M(r.salt),o=M(r.iv),s=M(r.ciphertext);if(A(e,i)!==r.address)throw Error(`Wallet vault belongs to another network or is corrupted.`);try{let n=await Ee(t,a),r=await crypto.subtle.decrypt({name:`AES-GCM`,iv:o,additionalData:De(e,i)},n,s);return we(e,await crypto.subtle.importKey(`pkcs8`,r,`Ed25519`,!1,[`sign`]),i)}catch{throw Error(`Incorrect password or corrupted wallet vault.`)}}function Ce(e){return typeof e==`object`&&!!e&&!Array.isArray(e)}function k(e,t){let n=e[t];if(typeof n!=`string`||n.length===0||n.length>1024)throw Error(`Wallet backup is malformed.`);return n}function we(e,t,n){let i=F(n),a=A(e,n);return{address:a,accountId:i,publicKey:n,createSession(e){if(!Number.isSafeInteger(e)||e<=Date.now())throw Error(`Wallet session expiry is invalid.`);return{version:1,address:a,publicKey:j(n),privateKey:t,expiresAtMs:e}},async createApprovalCodeIssue(e){if(e.deviceId.length!==32)throw Error(`Payrail Code deviceId must contain 32 bytes.`);let i=e.issuedAtMs??Date.now();if(!Number.isSafeInteger(i)||i<0)throw Error(`Payrail Code issue timestamp is invalid.`);let o=crypto.getRandomValues(new Uint8Array(32)),s=P(_e,r(e.networkId,32),n,e.deviceId,N(BigInt(i),8),o),c=new Uint8Array(await crypto.subtle.sign(`Ed25519`,t,s));return{accountAddress:a,deviceId:F(e.deviceId),issuedAtMs:i.toString(),nonce:F(o),signature:F(c)}},async signPhoneLinkChallenge(e){return ye(e),F(new Uint8Array(await crypto.subtle.sign(`Ed25519`,t,new TextEncoder().encode(e))))},async signTransfer(e){return Te(t,n,Ae(n,e))},async signContractDeploy(e){return Te(t,n,D(n,e))},async signContractCall(e){return Te(t,n,le(n,e))}}}async function Te(e,t,n){let r=P(fe,Uint8Array.of(0),n);return P(pe,n,t,new Uint8Array(await crypto.subtle.sign(`Ed25519`,e,r)),Uint8Array.of(0))}function A(e,t){return se.encode(e,se.toWords(P(Uint8Array.of(0),t)))}async function Ee(e,t){let n=await crypto.subtle.importKey(`raw`,new TextEncoder().encode(e),`PBKDF2`,!1,[`deriveKey`]);return crypto.subtle.deriveKey({name:`PBKDF2`,hash:`SHA-256`,salt:t,iterations:he},n,{name:`AES-GCM`,length:256},!1,[`encrypt`,`decrypt`])}function De(e,t){return new TextEncoder().encode(`${me}\0${e}\0${F(t)}`)}function Oe(e){if(e.length<10||e.length>256)throw Error(`Password must contain between 10 and 256 characters.`)}function j(e){let t=``;for(let n of e)t+=String.fromCharCode(n);return btoa(t)}function M(e){if(!/^[A-Za-z0-9+/]*={0,2}$/.test(e))throw Error(`Wallet vault contains invalid encoded data.`);let t=atob(e);return Uint8Array.from(t,e=>e.charCodeAt(0))}function ke(e,t){let n;try{n=se.decodeToBytes(e)}catch{throw Error(`Enter a valid payment address.`)}if(n.prefix!==t)throw Error(`This address is not for the ${t} network.`);if(n.bytes.length!==33||n.bytes[0]!==ge)throw Error(`This address is not a supported account address.`);return F(n.bytes.slice(1))}function Ae(e,t){if(t.idempotencyKey.length!==32)throw Error(`Idempotency key must be 32 bytes.`);return P(Uint8Array.of(0),r(t.networkId,32),t.idempotencyKey,r(t.assetId,32),e,r(t.recipientAccountId,32),N(t.amount,16),N(t.fee,16),N(t.nonce,8),N(t.validUntilHeight,8))}function je(e){if(!e||!/^(?:0|[1-9]\d*)$/u.test(e))throw Error(`Phone-link amount is not canonical.`);let t=BigInt(e);if(t<=0n||t>9007199254740991n)throw Error(`Phone-link amount is outside the supported range.`);return t}function Me(e){if(!e||!/^(?:0|[1-9]\d*)$/u.test(e))throw Error(`Phone-link timestamp is not canonical.`);let t=Number(e);if(!Number.isSafeInteger(t)||t<=0)throw Error(`Phone-link timestamp is invalid.`);return t}function N(e,t){if(e<0n)throw Error(`Unsigned values cannot be negative.`);let n=new Uint8Array(t),r=e;for(let e=t-1;e>=0;--e)n[e]=Number(r&255n),r>>=8n;if(r!==0n)throw Error(`Value does not fit in ${t} bytes.`);return n}function P(...e){let t=e.reduce((e,t)=>e+t.length,0),n=new Uint8Array(t),r=0;for(let t of e)n.set(t,r),r+=t.length;return n}function F(e){return Array.from(e,e=>e.toString(16).padStart(2,`0`)).join(``)}var Ne=class{observers=new Set;onNoObservers(){}},I;function Pe(e){!I||I.dependencies.has(e)||(e.observers.add(I),I.dependencies.add(e))}function Fe(e){for(let t of e.dependencies)t.observers.delete(e),t.observers.size===0&&t.onNoObservers();e.dependencies.clear()}function Ie(e){for(let t of[...e.observers])t.onDependencyChange()}var Le=class extends Ne{#e;constructor(e){super(),this.#e=e}get value(){return Pe(this),this.#e}set value(e){Object.is(this.#e,e)||(this.#e=e,Ie(this))}update(e){this.value=e(this.#e)}},Re=class extends Ne{derive;dependencies=new Set;#e=!0;#t=!1;#n;constructor(e){super(),this.derive=e}get value(){return Pe(this),this.#e&&this.#r(),this.observers.size===0&&(Fe(this),this.#e=!0),this.#n}onNoObservers(){Fe(this),this.#e=!0}onDependencyChange(){this.#e||(this.#e=!0,Ie(this))}#r(){if(this.#t)throw Error(`Circular computed dependency`);this.#t=!0,this.#e=!1,Fe(this);let e=I;I=this;try{this.#n=this.derive()}catch(e){throw this.#e=!0,e}finally{I=e,this.#t=!1}}},L=new Set,ze=!1;function Be(e){e.active&&(L.add(e),!ze&&(ze=!0,queueMicrotask(Ve)))}function Ve(){ze=!1;let e=0;for(;L.size>0;){if(++e>1e3)throw L.clear(),Error(`Reactive update loop exceeded 1000 iterations`);let t=L.values().next().value;if(!t)break;L.delete(t),t.run()}}var He=class{callback;dependencies=new Set;active=!0;#e;constructor(e){this.callback=e}run(){if(!this.active)return;this.#e?.(),this.#e=void 0,Fe(this);let e=I;I=this;try{let e=this.callback();typeof e==`function`&&(this.#e=e)}finally{I=e}}onDependencyChange(){Be(this)}dispose(){this.active&&(this.active=!1,L.delete(this),Fe(this),this.#e?.(),this.#e=void 0)}};function R(e){return new Le(e)}function Ue(e){let t=new He(e);try{t.run()}catch(e){throw t.dispose(),e}return()=>t.dispose()}function We(e){return e instanceof Le||e instanceof Re}function Ge(e){if(!/^[a-z_:][a-z0-9_:.\-]*$/i.test(e)||/^on/i.test(e)||/^srcdoc$/i.test(e))throw TypeError(`Invalid or unsafe attribute name.`)}function Ke(e,t){if(t==null||t===!1)return null;if(typeof t!=`string`&&typeof t!=`number`&&typeof t!=`boolean`)throw TypeError(`Attribute ${e} must resolve to a string, number, or boolean.`);let n=t===!0?``:String(t);if(/^(href|src|action|formaction|xlink:href)$/i.test(e)){let t;try{t=new URL(n,`https://workstar.invalid`).protocol}catch{throw TypeError(`Invalid URL for attribute ${e}.`)}if(![`http:`,`https:`,`mailto:`,`tel:`].includes(t))throw TypeError(`Unsafe URL for attribute ${e}.`)}return n}var qe=Symbol(`workstar.template`),Je=Symbol(`workstar.directive`),Ye=Symbol(`workstar.repeat`),z=`workstar-slot-`,Xe=`data-workstar-directive-`,Ze=`workstar-array-`,Qe=`workstar-repeat-`;function $e(e,t,n){return{[Ye]:!0,source:e,key:t,view:n}}function et(e){return typeof e==`object`&&!!e&&Ye in e}function tt(e){let t=B(B(e.source));if(!t||typeof t[Symbol.iterator]!=`function`)throw TypeError(`A repeat source must be iterable.`);let n=new Set;return Array.from(t,t=>{let r=e.key(t);if(typeof r!=`string`&&typeof r!=`number`||typeof r==`number`&&!Number.isFinite(r))throw TypeError(`A repeat key must be a string or finite number.`);if(n.has(r))throw Error(`Duplicate repeat key: ${String(r)}.`);return n.add(r),{key:r,item:t}})}function nt(e){return typeof e==`object`&&!!e&&qe in e}function rt(e){return typeof e==`object`&&!!e&&Je in e}function B(e){return We(e)?e.value:typeof e==`function`?e():e}function V(e,...t){return{[qe]:!0,strings:e,values:t}}function H(e,t,n){if(!/^[a-z][a-z0-9:-]*$/i.test(e))throw TypeError(`Invalid event name.`);return{[Je]:!0,kind:`event`,event:e,listener:t,...n===void 0?{}:{options:n}}}function U(e,t){return Ge(e),{[Je]:!0,kind:`attribute`,name:e,source:t}}function it(e){return{[Je]:!0,kind:`element-ref`,set:e}}function at(e){let t=B(e);if(!t||typeof t!=`object`||Array.isArray(t)||![Object.prototype,null].includes(Object.getPrototypeOf(t)))throw TypeError(`Attribute spread must resolve to a plain record.`);let n=[];for(let[e,r]of Object.entries(t)){if(Ge(e),/^(?:style|children|ref|key|className|htmlFor|innerHTML)$/i.test(e)||e.startsWith(`data-workstar-directive-`))throw TypeError(`Unsupported spread attribute ${e}.`);let t=Ke(e,r===!1&&/^(?:aria|data)-/i.test(e)?`false`:r);t!==null&&n.push([e,t])}return n}function ot(e){let t=B(e);if(t==null||t===!1)return``;if(typeof t==`string`||typeof t==`number`||typeof t==`bigint`)return String(t);throw TypeError(`Textarea value must be text or a number.`)}var st=new WeakMap,ct=class{#e=[];own(e){this.#e.push(e)}dispose(){for(let e of this.#e.splice(0).reverse())e()}};function lt(e,t,n){if(e==null||e===!1)return t.createDocumentFragment();if(et(e)){let r=t.createDocumentFragment(),i=t.createComment(`workstar-repeat`),a=t.createComment(`/workstar-repeat`);return r.append(i,a),ht(i,a,e,n,t),r}if(nt(e))return xt(e,t,n);if(Array.isArray(e)){let r=t.createDocumentFragment();for(let i of e)r.append(lt(i,t,n));return r}if(e instanceof Node)return e;if(typeof e==`string`||typeof e==`number`||typeof e==`bigint`)return t.createTextNode(String(e));throw TypeError(`Unsupported template value. Use text, a DOM node, html, or an array.`)}function ut(e,t,n){for(;e.nextSibling&&e.nextSibling!==t;)e.nextSibling.remove();t.parentNode?.insertBefore(n,t)}function dt(e,t,n,r,i,a=!1){if(et(n)){ht(e,t,n,r,i,a);return}let o,s=a,c,l,u=!1;r.own(()=>c?.dispose()),r.own(Ue(()=>{let r=B(n);if(s){s=!1;let n=new ct;try{vt(r,e,t,n,i),o=e.nextSibling?.nodeType===Node.TEXT_NODE&&e.nextSibling.nextSibling===t?e.nextSibling:void 0}catch(e){throw n.dispose(),e}c=n,l=r,u=!0;return}if(u&&Object.is(l,r))return;if(o&&e.nextSibling===o&&o.nextSibling===t&&(typeof r==`string`||typeof r==`number`||typeof r==`bigint`)){o.data=String(r),l=r,u=!0;return}let a=new ct;try{let n=lt(r,i,a);o=n.nodeType===Node.TEXT_NODE?n:void 0,c?.dispose(),ut(e,t,n)}catch(e){throw a.dispose(),e}c=a,l=r,u=!0}))}function ft(e,t){let n=e.data;if(!n.startsWith(t))throw Error(`Invalid hydration marker.`);let r=`/${n}`,i=0;for(let t=e.nextSibling;t;t=t.nextSibling){if(t.nodeType!==Node.COMMENT_NODE)continue;let e=t;if(e.data===n&&i++,e.data===r){if(i===0)return e;i--}}throw Error(`Missing hydration marker ${r}.`)}function pt(e,t){if(e.end.nextSibling===t)return;let n=e.start.ownerDocument.createDocumentFragment();for(let t=e.start;t;){let r=t.nextSibling;if(n.appendChild(t),t===e.end)break;t=r}t.parentNode?.insertBefore(n,t)}function mt(e){e.scope.dispose();for(let t=e.start;t;){let n=t.nextSibling;if(t.parentNode?.removeChild(t),t===e.end)break;t=n}}function ht(e,t,n,r,i,a=!1){let o=new Map;r.own(()=>{for(let e of o.values())e.scope.dispose();o.clear()});let s=a;r.own(Ue(()=>{let r=tt(n);if(s){s=!1;let a=e.nextSibling,c=new Map;o=c;for(let[e,{key:t,item:o}]of r.entries()){if(a?.nodeType!==Node.COMMENT_NODE||a.data!==`workstar-repeat-${e}`)throw Error(`Missing hydration repeat item ${e}.`);let r=a,s=ft(r,Qe),l=new ct,u=R(o);c.set(t,{start:r,end:s,scope:l,item:u}),dt(r,s,n.view(u),l,i,!0),a=s.nextSibling}if(a!==t)throw Error(`Hydration repeat length mismatch.`);return}let a=new Map;try{for(let{key:e,item:s}of r){let r=o.get(e);if(r){r.item.value=s,a.set(e,r);continue}let c=new ct,l=R(s),u=i.createComment(`${Qe}${a.size}`),d=i.createComment(`/${Qe}${a.size}`),f=i.createDocumentFragment();f.append(u,d);let p={start:u,end:d,scope:c,item:l};a.set(e,p),dt(u,d,n.view(l),c,i),t.parentNode?.insertBefore(f,t)}for(let[e,t]of o)a.has(e)||mt(t);let e=t;for(let t of[...a.values()].reverse())pt(t,e),e=t.start;o=a}catch(e){for(let[e,t]of a)o.has(e)||mt(t);throw e}}))}function gt(e,t,n,r){for(let i=e;i&&i!==t;i=i.nextSibling){if(i.nodeType===Node.COMMENT_NODE){let e=i;if(e.data.startsWith(`workstar-slot-`)){let t=Number(e.data.slice(z.length));if(!Number.isInteger(t)||n.has(t))throw Error(`Invalid or duplicate hydration slot.`);let r=ft(e,z);n.set(t,[e,r]),i=r}continue}if(i.nodeType!==Node.ELEMENT_NODE)continue;let e=i;for(let t of e.getAttributeNames()){if(!t.startsWith(`data-workstar-directive-`))continue;let n=Number(t.slice(Xe.length));if(!Number.isInteger(n)||r.has(n))throw Error(`Invalid or duplicate hydration directive.`);r.set(n,e)}gt(e.firstChild,null,n,r)}}function _t(e,t,n,r,i){let a=new Map,o=new Map;gt(t,n,a,o);let s=e.values.filter(e=>!rt(e)),c=e.values.length-s.length;if(a.size!==s.length||o.size!==c)throw Error(`Hydration markers do not match the template.`);for(let[t,n]of e.values.entries())if(rt(n)){let e=o.get(t);if(!e)throw Error(`Missing hydration directive ${t}.`);bt(e,n,r,!0),e.removeAttribute(`${Xe}${t}`)}else{let e=a.get(t);if(!e)throw Error(`Missing hydration slot ${t}.`);dt(e[0],e[1],n,r,i,!0)}}function vt(e,t,n,r,i){if(et(e)){ht(t,n,e,r,i,!0);return}if(nt(e)){_t(e,t.nextSibling,n,r,i);return}if(Array.isArray(e)){let a=t.nextSibling;for(let[t,n]of e.entries()){if(a?.nodeType!==Node.COMMENT_NODE||a.data!==`workstar-array-${t}`)throw Error(`Missing hydration array item ${t}.`);let e=a,o=ft(e,Ze);vt(B(n),e,o,r,i),a=o.nextSibling}if(a!==n)throw Error(`Hydration array length mismatch.`);return}if(e==null||e===!1){if(t.nextSibling!==n)throw Error(`Hydration content mismatch.`);return}if(typeof e==`string`||typeof e==`number`||typeof e==`bigint`){let r=String(e),i=t.nextSibling;if(r===``&&i===n)return;if(i?.nodeType!==Node.TEXT_NODE||i.nextSibling!==n||i.data!==r)throw Error(`Hydration text mismatch.`);return}throw TypeError(`Unsupported hydration value.`)}function yt(e,t,n){let r=Ke(t,n);if(r===null){e.removeAttribute(t);return}e.setAttribute(t,r)}function bt(e,t,n,r=!1){if(t.kind===`element-ref`){t.set(e),n.own(()=>t.set(null));return}if(t.kind===`event`){e.addEventListener(t.event,t.listener,t.options),n.own(()=>e.removeEventListener(t.event,t.listener,t.options));return}if(t.kind===`textarea`){if(!(e instanceof HTMLTextAreaElement))throw TypeError(`Textarea value directive requires a textarea element.`);let i=!0;n.own(Ue(()=>{let n=ot(t.source);if(i&&r){if(e.defaultValue!==n)throw Error(`Hydration textarea value mismatch.`);if(e.value!==e.defaultValue){i=!1;return}}e.value=n,i=!1}));return}if(t.kind===`attributes`){let r=new Set;n.own(Ue(()=>{let n=at(t.source),i=new Set(n.map(([e])=>e));for(let t of r)i.has(t)||e.removeAttribute(t);for(let[t,r]of n)e.setAttribute(t,r);r=i}));return}n.own(Ue(()=>yt(e,t.name,B(t.source))))}function xt(e,t,n){let r=e.strings[0]??``;for(let t=0;t<e.values.length;t++){let n=rt(e.values[t])?` ${Xe}${t}=""`:`<!--${z}${t}-->`;r+=n+(e.strings[t+1]??``)}let i=t.createElement(`template`);i.innerHTML=r;let a=i.content,o=new Map,s=new Map,c=t.createTreeWalker(a,NodeFilter.SHOW_COMMENT);for(;c.nextNode();){let e=c.currentNode;if(!e.data.startsWith(`workstar-slot-`))continue;let t=Number(e.data.slice(z.length));Number.isInteger(t)&&o.set(t,e)}let l=t.createTreeWalker(a,NodeFilter.SHOW_ELEMENT);for(;l.nextNode();){let e=l.currentNode;for(let t of e.getAttributeNames()){if(!t.startsWith(`data-workstar-directive-`))continue;let n=Number(t.slice(Xe.length));Number.isInteger(n)&&s.set(n,e),e.removeAttribute(t)}}for(let[r,i]of e.values.entries())if(rt(i)){let e=s.get(r);if(!e)throw Error(`Directives must be placed inside an opening HTML tag.`);bt(e,i,n)}else{let e=o.get(r);if(!e||!e.parentNode)throw Error(`Dynamic attributes require attr(). Child expressions belong in element content.`);let a=t.createComment(`/${z}${r}`);e.parentNode.insertBefore(a,e.nextSibling),dt(e,a,i,n,t)}return a}function St(e,t){st.get(e)?.();let n=e.ownerDocument,r=new ct,i=n.createComment(`workstar-root`),a=n.createComment(`/workstar-root`);e.replaceChildren(i,a);try{dt(i,a,t,r,n)}catch(t){throw r.dispose(),e.replaceChildren(),t}let o=!0,s=()=>{o&&(o=!1,st.delete(e),r.dispose(),e.replaceChildren())};return st.set(e,s),s}function Ct(e={}){return V`<a
    class="pr-brand"
    ${U(`href`,e.href??`https://payrail.one`)}
    ${U(`aria-label`,e.label??`Payrail home`)}
  >
    <span class="pr-brand__mark" aria-hidden="true"><i></i><i></i></span>
    <span class="pr-brand__word">payrail</span>
    ${e.suffix?V`<span class="pr-brand__suffix">${e.suffix}</span>`:null}
  </a>`}var wt=`/api`,W=new t(wt);function Tt(e){let t=new URL(`${wt.replace(/\/$/,``)}/live`,location.href);return t.protocol=t.protocol===`https:`?`wss:`:`ws:`,t.searchParams.set(`address`,e),t.toString()}var Et=`payrail-wallet`,Dt=`vaults`,Ot=`primary`,kt=`session`;async function At(){return Ft(`readonly`,e=>e.get(Ot))}async function jt(e){await Ft(`readwrite`,t=>t.put(e,Ot))}async function Mt(){return Ft(`readonly`,e=>e.get(kt))}async function Nt(e){await Ft(`readwrite`,t=>t.put(e,kt))}async function Pt(){await Ft(`readwrite`,e=>e.delete(kt))}function Ft(e,t){return new Promise((n,r)=>{let i=indexedDB.open(Et,1);i.onupgradeneeded=()=>{let e=i.result;e.objectStoreNames.contains(Dt)||e.createObjectStore(Dt)},i.onerror=()=>r(Error(`Unable to open wallet storage.`)),i.onsuccess=()=>{let a=i.result,o=a.transaction(Dt,e),s=t(o.objectStore(Dt));s.onsuccess=()=>n(s.result),s.onerror=()=>r(Error(`Unable to access wallet storage.`)),o.oncomplete=()=>a.close(),o.onerror=()=>a.close()}})}function It(e){return V`<section class="auth-layout">
    <div class="hero">
      <p class="eyebrow">Payrail self-custody</p>
      <h1>Your money.<br /><em>Your signature.</em></h1>
      <p>Keys are encrypted on this device and never sent to Payrail.</p>
      <div class="trust-row">
        <span>600K PBKDF2 rounds</span><span>AES-256-GCM</span
        ><span>Client-side Ed25519</span>
      </div>
      <div class="trust-flow" aria-label="Payrail security model">
        <div>
          <b>01</b
          ><span
            ><strong>Created here</strong
            ><small>Your key begins on this device</small></span
          >
        </div>
        <i aria-hidden="true"></i>
        <div>
          <b>02</b
          ><span
            ><strong>Signed locally</strong
            ><small>Only the operation leaves</small></span
          >
        </div>
        <i aria-hidden="true"></i>
        <div>
          <b>03</b
          ><span
            ><strong>Finalized onchain</strong
            ><small>Independently verifiable</small></span
          >
        </div>
      </div>
    </div>
    <article class="auth-card">
      <p class="eyebrow">
        ${()=>e.vault.value?`Welcome back`:`New wallet`}
      </p>
      <h2>${()=>e.vault.value?`Unlock wallet`:`Create wallet`}</h2>
      <form data-testid="auth-form" ${H(`submit`,t=>void(e.vault.value?e.unlock(t):e.createWallet(t)))}>
        <label
          >Password<input
            name="password"
            data-testid="password"
            type="password"
            minlength="10"
            maxlength="256"
            autocomplete="current-password"
            required
        /></label>
        ${()=>e.vault.value?null:V`<label
                >Confirm password<input
                  name="confirmation"
                  data-testid="password-confirmation"
                  type="password"
                  minlength="10"
                  maxlength="256"
                  autocomplete="new-password"
                  required
              /></label>`}
        <button
          class="primary"
          data-testid="authorize-wallet"
          type="submit"
          ${U(`disabled`,e.busy)}
        >
          ${()=>e.vault.value?`Unlock securely`:`Create encrypted wallet`}
        </button>
      </form>
      <p class="status" role="status">${e.message}</p>
      <div class="restore-intro">
        <span></span><small>New device or existing wallet?</small><span></span>
      </div>
      <label class="restore-vault">
        <strong>Restore encrypted backup</strong>
        <small
          >Import your JSON vault, then unlock with the same password.</small
        >
        <input
          data-testid="import-wallet"
          type="file"
          accept="application/json,.json"
          ${U(`disabled`,e.busy)}
          ${H(`change`,t=>void e.importVault(t))}
        />
      </label>
    </article>
  </section>`}var Lt=`0xaa36a7`,Rt=`Ethereum Sepolia`,G=`0x5e3a1bBB81d5C3c1391796E4e3c8c4E0420a8c4a`,zt=`0x74E345a36BCf50B253bac512C322d1a8Abc6b298`,Bt=`095ea7b3`,Vt=`dd62ed3e`,Ht=`70a08231`,Ut=`1de26e16`,Wt=(1n<<256n)-1n;function Gt(){let e=window.ethereum;return rn(e)?e:null}async function Kt(e){let t=await e.request({method:`eth_requestAccounts`});if(await e.request({method:`wallet_switchEthereumChain`,params:[{chainId:Lt}]}),await e.request({method:`eth_chainId`})!==`0xaa36a7`)throw Error(`MetaMask must be connected to Ethereum Sepolia.`);let n=nn(t);if(!n)throw Error(`MetaMask did not return an Ethereum account.`);return n}async function qt(e,t){return Xt(e,G,Ht+$t(t))}async function Jt(e,t,n,r,i){Qt(r);let a=en(n),o=await Xt(e,G,Vt+$t(t)+$t(zt));return o<r&&(i(`approving`),o!==0n&&await Zt(e,t,G,Yt(0n)),await Zt(e,t,G,Yt(r))),i(`depositing`),Zt(e,t,zt,`0x${Ut}${a}${tn(r)}`)}function Yt(e){return`0x${Bt}${$t(zt)}${tn(e)}`}async function Xt(e,t,n){let r=await e.request({method:`eth_call`,params:[{to:t,data:`0x${n}`},`latest`]});if(typeof r!=`string`||!/^0x[0-9a-fA-F]{1,64}$/.test(r))throw Error(`Ethereum returned an invalid contract value.`);return BigInt(r)}async function Zt(e,t,n,r){let i=await e.request({method:`eth_sendTransaction`,params:[{from:t,to:n,data:r}]});if(typeof i!=`string`||!/^0x[0-9a-fA-F]{64}$/.test(i))throw Error(`MetaMask returned an invalid transaction hash.`);for(let t=0;t<120;t+=1){let t=await e.request({method:`eth_getTransactionReceipt`,params:[i]});if(t!==null){if(!an(t)||t.status!==`0x1`)throw Error(`The Ethereum transaction reverted.`);return i}await on(1500)}throw Error(`Ethereum confirmation timed out. Check the transaction in Etherscan.`)}function Qt(e){if(e<=0n||e>Wt)throw Error(`Enter a positive tUSDT amount.`)}function $t(e){if(!/^0x[0-9a-fA-F]{40}$/.test(e))throw Error(`Ethereum returned an invalid account address.`);return e.slice(2).toLowerCase().padStart(64,`0`)}function en(e){if(!/^[0-9a-f]{64}$/.test(e))throw Error(`Payrail account ID is not canonical.`);return e}function tn(e){if(e<0n||e>Wt)throw Error(`Ethereum amount is outside uint256.`);return e.toString(16).padStart(64,`0`)}function nn(e){return!Array.isArray(e)||typeof e[0]!=`string`?null:/^0x[0-9a-fA-F]{40}$/.test(e[0])?e[0]:null}function rn(e){return an(e)&&typeof e.request==`function`}function an(e){return typeof e==`object`&&!!e&&!Array.isArray(e)}function on(e){return new Promise(t=>window.setTimeout(t,e))}function sn(e){let t=R(null),n=R(`0`),r=R(null),i=R(null),o=R(`idle`),s=R(!1),c=null,l=async()=>{let i=e.wallet.value;if(!i)return;let[a,o]=await Promise.all([W.bridgeAccount(i.address),c&&t.value?qt(c,t.value):Promise.resolve(null)]);r.value=a,o!==null&&(n.value=o.toString())};return{ethereumAddress:t,ethereumBalance:n,payrailBalance:r,transactionHash:i,stage:o,busy:s,providerAvailable:Gt()!==null,connect:async()=>{s.value=!0,o.value=`connecting`;try{if(c=Gt(),!c)throw Error(`Install MetaMask to bridge from Sepolia.`);t.value=await Kt(c),await l(),o.value=`idle`,e.setMessage(`MetaMask connected to Ethereum Sepolia.`)}catch(t){o.value=`idle`,e.setMessage(ln(t))}finally{s.value=!1}},deposit:async l=>{l.preventDefault();let u=e.wallet.value,d=e.status.value;if(!(!u||!d)){s.value=!0,i.value=null;try{if(!c||!t.value)throw Error(`Connect MetaMask before depositing.`);let s=new FormData(l.currentTarget).get(`bridge-amount`);if(typeof s!=`string`)throw Error(`Enter a tUSDT amount.`);let f=a(s,d.bridgeAsset?.decimals??6);if(f>await qt(c,t.value))throw Error(`Amount exceeds the MetaMask tUSDT balance.`);let p=BigInt(r.value?.balance??`0`);i.value=await Jt(c,t.value,u.accountId,f,t=>{o.value=t,e.setMessage(t===`approving`?`Confirm the tUSDT approval in MetaMask.`:`Confirm the vault deposit in MetaMask.`)}),o.value=`waiting`,e.setMessage(`Deposit mined. Waiting for finalized Ethereum consensus and Payrail settlement…`);let m=await cn(u.address,p+f);r.value=m,n.value=(await qt(c,t.value)).toString(),o.value=`complete`,e.setMessage(`tUSDT finalized on Payrail at block #${m.finalizedHeight}.`)}catch(t){o.value!==`waiting`&&(o.value=`idle`),e.setMessage(ln(t))}finally{s.value=!1}}},refresh:l}}async function cn(e,t){for(let n=0;n<180;n+=1){let n=await W.bridgeAccount(e);if(BigInt(n.balance)>=t)return n;await new Promise(e=>window.setTimeout(e,1e4))}throw Error(`Deposit is still waiting for Ethereum finality. You can safely refresh later.`)}function ln(e){return e instanceof Error||typeof e==`object`&&e&&`message`in e&&typeof e.message==`string`?e.message:`Bridge request failed.`}var K=(e=>(e[e.Border=-1]=`Border`,e[e.Data=0]=`Data`,e[e.Function=1]=`Function`,e[e.Position=2]=`Position`,e[e.Timing=3]=`Timing`,e[e.Alignment=4]=`Alignment`,e))(K||{}),un=[0,1],dn=[1,0],fn=[2,3],pn=[3,2],mn={L:un,M:dn,Q:fn,H:pn},hn=/^\d*$/,gn=/^[A-Z0-9 $%*+./:-]*$/,_n=`0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:`,vn=1,yn=40,bn=3,xn=3,Sn=40,Cn=10,wn=[[-1,7,10,15,20,26,18,20,24,30,18,20,24,26,30,22,24,28,30,28,28,28,28,30,30,26,28,30,30,30,30,30,30,30,30,30,30,30,30,30,30],[-1,10,16,26,18,24,16,18,22,22,26,30,22,22,24,24,28,28,26,26,26,26,28,28,28,28,28,28,28,28,28,28,28,28,28,28,28,28,28,28,28],[-1,13,22,18,26,18,24,18,22,20,24,28,26,24,20,30,24,28,28,26,30,28,30,30,30,30,28,30,30,30,30,30,30,30,30,30,30,30,30,30,30],[-1,17,28,22,16,22,28,26,26,24,28,24,28,22,24,24,30,28,28,26,28,30,24,30,30,30,30,30,30,30,30,30,30,30,30,30,30,30,30,30,30]],Tn=[[-1,1,1,1,1,1,2,2,2,2,4,4,4,4,4,6,6,6,6,7,8,8,9,9,10,12,12,12,13,14,15,16,17,18,19,19,20,21,22,24,25],[-1,1,1,1,2,2,4,4,4,5,5,5,8,9,9,10,10,11,13,14,16,17,17,18,20,21,23,25,26,28,29,31,33,35,37,38,40,43,45,47,49],[-1,1,1,2,2,4,4,6,6,8,8,8,10,12,16,12,17,16,18,21,20,23,23,25,27,29,34,34,35,38,40,43,45,48,51,53,56,59,62,65,68],[-1,1,1,2,4,4,4,5,6,8,8,11,11,16,16,18,16,19,21,25,25,25,34,30,32,35,37,40,42,45,48,51,54,57,60,63,66,70,74,77,81]],En=class{constructor(e,t,n,r){if(this.version=e,this.ecc=t,e<vn||e>yn)throw RangeError(`Version value out of range`);if(r<-1||r>7)throw RangeError(`Mask value out of range`);this.size=e*4+17;let i=Array.from({length:this.size}).fill(!1);for(let e=0;e<this.size;e++)this.modules.push(i.slice()),this.types.push(i.map(()=>0));this.drawFunctionPatterns();let a=this.addEccAndInterleave(n);if(this.drawCodewords(a),r===-1){let e=1e9;for(let t=0;t<8;t++){this.applyMask(t),this.drawFormatBits(t);let n=this.getPenaltyScore();n<e&&(r=t,e=n),this.applyMask(t)}}this.mask=r,this.applyMask(r),this.drawFormatBits(r)}size;mask;modules=[];types=[];getModule(e,t){return e>=0&&e<this.size&&t>=0&&t<this.size&&this.modules[t][e]}drawFunctionPatterns(){for(let e=0;e<this.size;e++)this.setFunctionModule(6,e,e%2==0,K.Timing),this.setFunctionModule(e,6,e%2==0,K.Timing);this.drawFinderPattern(3,3),this.drawFinderPattern(this.size-4,3),this.drawFinderPattern(3,this.size-4);let e=this.getAlignmentPatternPositions(),t=e.length;for(let n=0;n<t;n++)for(let r=0;r<t;r++)n===0&&r===0||n===0&&r===t-1||n===t-1&&r===0||this.drawAlignmentPattern(e[n],e[r]);this.drawFormatBits(0),this.drawVersion()}drawFormatBits(e){let t=this.ecc[1]<<3|e,n=t;for(let e=0;e<10;e++)n=n<<1^(n>>>9)*1335;let r=(t<<10|n)^21522;for(let e=0;e<=5;e++)this.setFunctionModule(8,e,J(r,e));this.setFunctionModule(8,7,J(r,6)),this.setFunctionModule(8,8,J(r,7)),this.setFunctionModule(7,8,J(r,8));for(let e=9;e<15;e++)this.setFunctionModule(14-e,8,J(r,e));for(let e=0;e<8;e++)this.setFunctionModule(this.size-1-e,8,J(r,e));for(let e=8;e<15;e++)this.setFunctionModule(8,this.size-15+e,J(r,e));this.setFunctionModule(8,this.size-8,!0)}drawVersion(){if(this.version<7)return;let e=this.version;for(let t=0;t<12;t++)e=e<<1^(e>>>11)*7973;let t=this.version<<12|e;for(let e=0;e<18;e++){let n=J(t,e),r=this.size-11+e%3,i=Math.floor(e/3);this.setFunctionModule(r,i,n),this.setFunctionModule(i,r,n)}}drawFinderPattern(e,t){for(let n=-4;n<=4;n++)for(let r=-4;r<=4;r++){let i=Math.max(Math.abs(r),Math.abs(n)),a=e+r,o=t+n;a>=0&&a<this.size&&o>=0&&o<this.size&&this.setFunctionModule(a,o,i!==2&&i!==4,K.Position)}}drawAlignmentPattern(e,t){for(let n=-2;n<=2;n++)for(let r=-2;r<=2;r++)this.setFunctionModule(e+r,t+n,Math.max(Math.abs(r),Math.abs(n))!==1,K.Alignment)}setFunctionModule(e,t,n,r=K.Function){this.modules[t][e]=n,this.types[t][e]=r}addEccAndInterleave(e){let t=this.version,n=this.ecc;if(e.length!==Vn(t,n))throw RangeError(`Invalid argument`);let r=Tn[n[0]][t],i=wn[n[0]][t],a=Math.floor(Bn(t)/8),o=r-a%r,s=Math.floor(a/r),c=[],l=Hn(i);for(let t=0,n=0;t<r;t++){let r=e.slice(n,n+s-i+(t<o?0:1));n+=r.length;let a=Un(r,l);t<o&&r.push(0),c.push(r.concat(a))}let u=[];for(let e=0;e<c[0].length;e++)c.forEach((t,n)=>{(e!==s-i||n>=o)&&u.push(t[e])});return u}drawCodewords(e){if(e.length!==Math.floor(Bn(this.version)/8))throw RangeError(`Invalid argument`);let t=0;for(let n=this.size-1;n>=1;n-=2){n===6&&(n=5);for(let r=0;r<this.size;r++)for(let i=0;i<2;i++){let a=n-i,o=n+1&2?r:this.size-1-r;!this.types[o][a]&&t<e.length*8&&(this.modules[o][a]=J(e[t>>>3],7-(t&7)),t++)}}}applyMask(e){if(e<0||e>7)throw RangeError(`Mask value out of range`);for(let t=0;t<this.size;t++)for(let n=0;n<this.size;n++){let r;switch(e){case 0:r=(n+t)%2==0;break;case 1:r=t%2==0;break;case 2:r=n%3==0;break;case 3:r=(n+t)%3==0;break;case 4:r=(Math.floor(n/3)+Math.floor(t/2))%2==0;break;case 5:r=n*t%2+n*t%3==0;break;case 6:r=(n*t%2+n*t%3)%2==0;break;case 7:r=((n+t)%2+n*t%3)%2==0;break;default:throw Error(`Unreachable`)}!this.types[t][n]&&r&&(this.modules[t][n]=!this.modules[t][n])}}getPenaltyScore(){let e=0;for(let t=0;t<this.size;t++){let n=!1,r=0,i=[0,0,0,0,0,0,0];for(let a=0;a<this.size;a++)this.modules[t][a]===n?(r++,r===5?e+=bn:r>5&&e++):(this.finderPenaltyAddHistory(r,i),n||(e+=this.finderPenaltyCountPatterns(i)*Sn),n=this.modules[t][a],r=1);e+=this.finderPenaltyTerminateAndCount(n,r,i)*Sn}for(let t=0;t<this.size;t++){let n=!1,r=0,i=[0,0,0,0,0,0,0];for(let a=0;a<this.size;a++)this.modules[a][t]===n?(r++,r===5?e+=bn:r>5&&e++):(this.finderPenaltyAddHistory(r,i),n||(e+=this.finderPenaltyCountPatterns(i)*Sn),n=this.modules[a][t],r=1);e+=this.finderPenaltyTerminateAndCount(n,r,i)*Sn}for(let t=0;t<this.size-1;t++)for(let n=0;n<this.size-1;n++){let r=this.modules[t][n];r===this.modules[t][n+1]&&r===this.modules[t+1][n]&&r===this.modules[t+1][n+1]&&(e+=xn)}let t=0;for(let e of this.modules)t=e.reduce((e,t)=>e+ +!!t,t);let n=this.size*this.size,r=Math.ceil(Math.abs(t*20-n*10)/n)-1;return e+=r*Cn,e}getAlignmentPatternPositions(){if(this.version===1)return[];{let e=Math.floor(this.version/7)+2,t=this.version===32?26:Math.ceil((this.version*4+4)/(e*2-2))*2,n=[6];for(let r=this.size-7;n.length<e;r-=t)n.splice(1,0,r);return n}}finderPenaltyCountPatterns(e){let t=e[1],n=t>0&&e[2]===t&&e[3]===t*3&&e[4]===t&&e[5]===t;return(n&&e[0]>=t*4&&e[6]>=t?1:0)+(n&&e[6]>=t*4&&e[0]>=t?1:0)}finderPenaltyTerminateAndCount(e,t,n){return e&&(this.finderPenaltyAddHistory(t,n),t=0),t+=this.size,this.finderPenaltyAddHistory(t,n),this.finderPenaltyCountPatterns(n)}finderPenaltyAddHistory(e,t){t[0]===0&&(e+=this.size),t.pop(),t.unshift(e)}};function q(e,t,n){if(t<0||t>31||e>>>t)throw RangeError(`Value out of range`);for(let r=t-1;r>=0;r--)n.push(e>>>r&1)}function J(e,t){return(e>>>t&1)!=0}var Dn=class{constructor(e,t,n){if(this.mode=e,this.numChars=t,this.bitData=n,t<0)throw RangeError(`Invalid argument`);this.bitData=n.slice()}getData(){return this.bitData.slice()}},On=[1,10,12,14],kn=[2,9,11,13],An=[4,8,16,16];function jn(e,t){return e[Math.floor((t+7)/17)+1]}function Mn(e){let t=[];for(let n of e)q(n,8,t);return new Dn(An,e.length,t)}function Nn(e){if(!In(e))throw RangeError(`String contains non-numeric characters`);let t=[];for(let n=0;n<e.length;){let r=Math.min(e.length-n,3);q(Number.parseInt(e.substring(n,n+r),10),r*3+1,t),n+=r}return new Dn(On,e.length,t)}function Pn(e){if(!Ln(e))throw RangeError(`String contains unencodable characters in alphanumeric mode`);let t=[],n;for(n=0;n+2<=e.length;n+=2){let r=_n.indexOf(e.charAt(n))*45;r+=_n.indexOf(e.charAt(n+1)),q(r,11,t)}return n<e.length&&q(_n.indexOf(e.charAt(n)),6,t),new Dn(kn,e.length,t)}function Fn(e){return e===``?[]:In(e)?[Nn(e)]:Ln(e)?[Pn(e)]:[Mn(zn(e))]}function In(e){return hn.test(e)}function Ln(e){return gn.test(e)}function Rn(e,t){let n=0;for(let r of e){let e=jn(r.mode,t);if(r.numChars>=1<<e)return 1/0;n+=4+e+r.bitData.length}return n}function zn(e){e=encodeURI(e);let t=[];for(let n=0;n<e.length;n++)e.charAt(n)===`%`?(t.push(Number.parseInt(e.substring(n+1,n+3),16)),n+=2):t.push(e.charCodeAt(n));return t}function Bn(e){if(e<vn||e>yn)throw RangeError(`Version number out of range`);let t=(16*e+128)*e+64;if(e>=2){let n=Math.floor(e/7)+2;t-=(25*n-10)*n-55,e>=7&&(t-=36)}return t}function Vn(e,t){return Math.floor(Bn(e)/8)-wn[t[0]][e]*Tn[t[0]][e]}function Hn(e){if(e<1||e>255)throw RangeError(`Degree out of range`);let t=[];for(let n=0;n<e-1;n++)t.push(0);t.push(1);let n=1;for(let r=0;r<e;r++){for(let e=0;e<t.length;e++)t[e]=Wn(t[e],n),e+1<t.length&&(t[e]^=t[e+1]);n=Wn(n,2)}return t}function Un(e,t){let n=t.map(e=>0);for(let r of e){let e=r^n.shift();n.push(0),t.forEach((t,r)=>n[r]^=Wn(t,e))}return n}function Wn(e,t){if(e>>>8||t>>>8)throw RangeError(`Byte out of range`);let n=0;for(let r=7;r>=0;r--)n=n<<1^(n>>>7)*285,n^=(t>>>r&1)*e;return n}function Gn(e,t,n=1,r=40,i=-1,a=!0){if(!(vn<=n&&n<=r&&r<=yn)||i<-1||i>7)throw RangeError(`Invalid value`);let o,s;for(o=n;;o++){let n=Vn(o,t)*8,i=Rn(e,o);if(i<=n){s=i;break}if(o>=r)throw RangeError(`Data too long`)}for(let e of[dn,fn,pn])a&&s<=Vn(o,e)*8&&(t=e);let c=[];for(let t of e){q(t.mode[0],4,c),q(t.numChars,jn(t.mode,o),c);for(let e of t.getData())c.push(e)}let l=Vn(o,t)*8;q(0,Math.min(4,l-c.length),c),q(0,(8-c.length%8)%8,c);for(let e=236;c.length<l;e^=253)q(e,8,c);let u=Array.from({length:Math.ceil(c.length/8)},()=>0);return c.forEach((e,t)=>u[t>>>3]|=e<<7-(t&7)),new En(o,t,u,i)}function Kn(e,t){let{ecc:n=`L`,boostEcc:r=!1,minVersion:i=1,maxVersion:a=40,maskPattern:o=-1,border:s=1}=t||{},c=typeof e==`string`?Fn(e):Array.isArray(e)?[Mn(e)]:void 0;if(!c)throw Error(`uqr only supports encoding string and binary data, but got: ${typeof e}`);let l=Gn(c,mn[n],i,a,o,r),u=qn({version:l.version,maskPattern:l.mask,size:l.size,data:l.modules,types:l.types},s);return t?.invert&&(u.data=u.data.map(e=>e.map(e=>!e))),t?.onEncoded?.(u),u}function qn(e,t=1){if(!t)return e;let{size:n}=e,r=n+t*2;e.size=r,e.data.forEach(e=>{for(let n=0;n<t;n++)e.unshift(!1),e.push(!1)});for(let n=0;n<t;n++)e.data.unshift(Array.from({length:r},e=>!1)),e.data.push(Array.from({length:r},e=>!1));let i=K.Border;e.types.forEach(e=>{for(let n=0;n<t;n++)e.unshift(i),e.push(i)});for(let n=0;n<t;n++)e.types.unshift(Array.from({length:r},e=>i)),e.types.push(Array.from({length:r},e=>i));return e}function Jn(e,t){return e&&t?`${o(e.balance,t.asset.decimals)} ${t.asset.symbol}`:`0 TEST`}function Y(e,t){return t?`${o(e,t.asset.decimals)} ${t.asset.symbol}`:e}function Yn(e){return e.length>24?`${e.slice(0,11)}…${e.slice(-9)}`:e}function Xn(e){return e?.finalityMode===`four-validator-quorum`?`Quorum finality`:e?.finalityMode===`bft-devnet`?`BFT finality`:`Devnet finality`}function Zn(e){let t=Kn(e,{ecc:`M`,border:4});return V`<canvas
    width="640"
    height="640"
    aria-label="QR payment request"
    ${it(e=>{if(!(e instanceof HTMLCanvasElement))return;let n=e.getContext(`2d`);if(!n)return;let r=640/t.size;n.fillStyle=`#fff`,n.fillRect(0,0,640,640),n.fillStyle=`#07100c`,t.data.forEach((e,t)=>e.forEach((e,i)=>{e&&n.fillRect(Math.floor(i*r),Math.floor(t*r),Math.ceil(r),Math.ceil(r))}))})}
  ></canvas>`}function Qn(e){return V`<article class="balance-card">
    <div class="card-orbit" aria-hidden="true"><i></i><i></i><i></i></div>
    <div class="card-head">
      <span>PAYRAIL</span>
      <div class="card-head-actions">
        <button
          type="button"
          ${U(`aria-pressed`,()=>String(e.balanceHidden.value))}
          ${U(`aria-label`,()=>e.balanceHidden.value?`Show balance`:`Hide balance`)}
          ${H(`click`,e.toggleBalancePrivacy)}
        >
          ${()=>e.balanceHidden.value?`Show`:`Hide`}</button
        ><b><i></i>DEVNET</b>
      </div>
    </div>
    <small>Available · finalized</small>
    <strong data-testid="wallet-balance"
      >${()=>e.balanceHidden.value?`••••••`:Jn(e.account.value,e.status.value)}</strong
    >
    <div class="card-address">
      <span>Public account</span>
      <code ${U(`title`,()=>e.wallet.value?.address??``)}
        >${()=>e.wallet.value?.address??`—`}</code
      >
    </div>
    <div class="card-foot">
      <span>ASSET ${()=>e.status.value?.asset.symbol??`—`}</span
      ><b>NETWORK FEE 0.00</b>
    </div>
  </article>`}function $n(e){return V`<div class="focused-layout code-layout">
    <section class="code-intro">
      <p class="eyebrow">Payrail Code · DEVNET</p>
      <h1>Six digits.<br />Your approval.</h1>
      <p>
        Generate a short-lived code, enter it at checkout, then return here to
        review the exact merchant and amount before signing.
      </p>
      <ul>
        <li>The code expires after two minutes</li>
        <li>It can link exactly one checkout</li>
        <li>It cannot move money without your signature</li>
      </ul>
    </section>
    <section class="payrail-code-card">
      ${()=>e.approvalCode.value?er(e):V`<div class="code-empty">
              <span class="code-mark">P</span>
              <h2>Ready when you are.</h2>
              <p>Create a one-time code inside this unlocked wallet.</p>
              <button
                class="primary"
                data-testid="create-payrail-code"
                type="button"
                ${U(`disabled`,e.busy)}
                ${H(`click`,()=>void e.createPayrailCode())}
              >
                Generate Payrail Code
              </button>
            </div>`}
    </section>
  </div>`}function er(e){return V`<div class="code-active" data-testid="payrail-code-active">
    <div class="code-status"><i></i>Waiting for a checkout</div>
    <div class="code-digits" data-testid="payrail-code">
      ${()=>[...e.approvalCode.value?.code??``].map(e=>V`<b>${e}</b>`)}
    </div>
    <div class="code-expiry">
      <span>Expires in</span
      ><strong>${()=>tr(e.codeSeconds.value)}</strong>
    </div>
    <p>
      Enter these digits only in a Payrail checkout. The payment details will
      appear here before anything is signed.
    </p>
    <div class="button-row">
      <button
        type="button"
        ${H(`click`,()=>void navigator.clipboard.writeText(e.approvalCode.value?.code??``))}
      >
        Copy code</button
      ><button
        class="secondary"
        type="button"
        ${H(`click`,e.cancelPayrailCode)}
      >
        Cancel
      </button>
    </div>
  </div>`}function tr(e){let t=Math.max(0,e);return`${Math.floor(t/60)}:${String(t%60).padStart(2,`0`)}`}function X(e){switch(e){case`identity`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="3" y="5" width="18" height="14" rx="3"></rect>
        <circle cx="8" cy="11" r="2"></circle>
        <path
          d="M5.5 16c.7-1.5 1.5-2.2 2.5-2.2s1.8.7 2.5 2.2M13 10h5M13 14h4"
        ></path>
      </svg>`;case`live`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="2.2"></circle>
        <path
          d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.6 4.6a10.5 10.5 0 0 0 0 14.8M19.4 4.6a10.5 10.5 0 0 1 0 14.8"
        ></path>
      </svg>`;case`notification`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4"
        ></path>
      </svg>`;case`security`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 3 20 6v5c0 5-3.3 8.3-8 10-4.7-1.7-8-5-8-10V6l8-3Z"></path>
        <path d="m8.5 12 2.2 2.2 4.8-5"></path>
      </svg>`;case`recovery`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="3" y="4" width="11" height="14" rx="2"></rect>
        <rect x="16" y="8" width="5" height="12" rx="1.5"></rect>
        <path d="m10 9 3 3-3 3M7 12h6"></path>
      </svg>`;case`send`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M5 19 19 5M10 5h9v9"></path>
      </svg>`;case`request`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 4v16M5 13l7 7 7-7"></path>
      </svg>`;case`explorer`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="8"></circle>
        <circle cx="12" cy="12" r="3"></circle>
        <path d="M12 2v2M22 12h-2M12 22v-2M2 12h2"></path>
      </svg>`;case`link`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="m9.5 14.5 5-5M7 17l-1 1a3.5 3.5 0 0 1-5-5l4-4a3.5 3.5 0 0 1 5 0M17 7l1-1a3.5 3.5 0 0 1 5 5l-4 4a3.5 3.5 0 0 1-5 0"
        ></path>
      </svg>`;case`qr`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM15 14h2v2h-2zM19 14h1v3h-3v3h-3v-2M19 20h1"
        ></path>
      </svg>`;case`sms`:return V`<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 5h16v12H9l-5 4V5ZM8 9h8M8 13h5"></path>
      </svg>`}}function nr(e){return V`<div class="request-layout">
      <section class="focused-copy">
        <p class="eyebrow">Omnichannel requests</p>
        <h1>One request.<br /><em>Every channel.</em></h1>
        <p>
          Create one immutable payment request for hosted web, QR and SMS. Each
          channel resolves to the same recipient and amount.
        </p>
        <div class="channel-preview" aria-label="Payment request channels">
          <div>
            <i>${X(`link`)}</i
            ><span
              ><strong>Hosted link</strong><small>Share anywhere</small></span
            >
          </div>
          <div>
            <i>${X(`qr`)}</i
            ><span
              ><strong>QR checkout</strong><small>Scan in person</small></span
            >
          </div>
          <div>
            <i>${X(`sms`)}</i
            ><span
              ><strong>Text message</strong
              ><small>Feature-phone ready</small></span
            >
          </div>
        </div>
      </section>
      <form
        class="request-builder"
        data-testid="request-form"
        ${H(`submit`,t=>void e.createCheckout(t))}
      >
        <p class="eyebrow">Payment link</p>
        <h2>Create request</h2>
        <label
          >Amount<input
            name="checkout-amount"
            data-testid="request-amount"
            inputmode="decimal"
            placeholder="12.50"
            required /></label
        ><label
          >Order reference<input
            name="order-reference"
            data-testid="request-reference"
            maxlength="64"
            pattern="[A-Za-z0-9_]+"
            placeholder="invoice_1042"
            required /></label
        ><button
          class="primary"
          data-testid="create-checkout"
          type="submit"
          ${U(`disabled`,e.busy)}
        >
          Create secure request
        </button>
      </form>
    </div>
    ${()=>e.checkout.value?rr(e.checkout.value,e.copySms):null}`}function rr(e,t){let n=`https://wallet.payrail.one${e.paymentPath}`,r=e.smsText.replace(`{origin}`,`https://wallet.payrail.one`);return V`<section class="channel-stage" data-testid="checkout-created">
    <article class="qr-card">
      <div>
        <p class="eyebrow">Scan to pay</p>
        <h2>${e.merchantLabel}</h2>
      </div>
      <div class="qr-frame">${Zn(n)}</div>
      <strong
        >${o(e.amount,e.asset.decimals)}
        ${e.asset.symbol}</strong
      ><a
        data-testid="checkout-link"
        ${U(`href`,n)}
        target="_blank"
        rel="noopener"
        >Open secure payment</a
      >
    </article>
    <article class="channel-card">
      <p class="eyebrow">Hosted checkout</p>
      <h2>Share one verified link</h2>
      <code>${n}</code
      ><button
        type="button"
        ${H(`click`,()=>void navigator.clipboard.writeText(n))}
      >
        Copy payment link
      </button>
    </article>
    <article class="channel-card sms-card">
      <p class="eyebrow">SMS delivery</p>
      <h2>Ready for feature phones</h2>
      <p data-testid="sms-preview">${r}</p>
      <div class="sms-actions">
        <button
          data-testid="sms-compose"
          type="button"
          ${H(`click`,()=>ir(r))}
        >
          Open SMS app</button
        ><button type="button" ${H(`click`,()=>void t())}>
          Copy message
        </button>
      </div>
    </article>
  </section>`}function ir(e){window.location.assign(`sms:?body=${encodeURIComponent(e)}`)}function ar(e){let t=e.checkout.value;if(!t)return V`<p class="empty-state">Loading verified payment request…</p>`;let n=t.status===`finalized`;return V`<section class="checkout-page">
    <article class="payment-sheet" data-testid="hosted-checkout">
      <div class="sheet-head">
        <div>
          <p class="eyebrow">Secure Payrail checkout</p>
          <h2>${t.merchantLabel}</h2>
        </div>
        <span>${t.status}</span>
      </div>
      <div class="checkout-amount">
        <small>Total</small
        ><strong
          >${o(t.amount,t.asset.decimals)}
          ${t.asset.symbol}</strong
        >
      </div>
      <dl>
        <div>
          <dt>Network fee</dt>
          <dd>0 ${t.asset.symbol}</dd>
        </div>
        <div>
          <dt>Network</dt>
          <dd>Payrail Devnet</dd>
        </div>
        <div>
          <dt>Recipient</dt>
          <dd>${Yn(t.merchantAddress)}</dd>
        </div>
        <div>
          <dt>Request</dt>
          <dd>${Yn(t.id)}</dd>
        </div>
      </dl>
      ${n?V`<div class="checkout-success" data-testid="checkout-finalized">
            <i>✓</i
            ><span
              ><strong>Payment finalized</strong
              ><small>${Yn(t.transaction?.id??``)}</small></span
            >
          </div>`:or(e,t)}
    </article>
  </section>`}function or(e,t){return BigInt(e.account.value?.balance??`0`)<BigInt(t.amount)?V`<div class="checkout-action">
      <p>Add test funds before paying this request.</p>
      <button
        class="primary"
        type="button"
        ${U(`disabled`,e.busy)}
        ${H(`click`,()=>void e.fund())}
      >
        Add test funds
      </button>
    </div>`:e.checkoutReviewed.value?V`<div class="checkout-action">
    <p>
      You are signing this exact amount and recipient. The private key remains
      on this device.
    </p>
    <div class="button-row">
      <button
        class="secondary"
        type="button"
        ${H(`click`,e.cancelCheckout)}
      >
        Cancel</button
      ><button
        class="primary"
        data-testid="confirm-checkout"
        type="button"
        ${U(`disabled`,e.busy)}
        ${H(`click`,()=>void e.confirmCheckout())}
      >
        Confirm & pay
      </button>
    </div>
  </div>`:V`<button
      class="primary full-button"
      data-testid="review-checkout"
      type="button"
      ${H(`click`,e.reviewCheckout)}
    >
      Review payment
    </button>`}function sr(e){return V`<div class="focused-layout send-layout">
    <section class="focused-copy">
      <p class="eyebrow">Signed transfer</p>
      <h1>Send with certainty.</h1>
      <p>
        The exact recipient, amount, network and nonce are signed locally.
        Payrail returns a deterministic final receipt.
      </p>
      ${Qn(e)}
    </section>
    <article class="send-panel">
      ${cr(e)}
      <p class="eyebrow">New payment</p>
      <h2>Send ${()=>e.status.value?.asset.symbol??`TEST`}</h2>
      <form
        data-testid="send-form"
        ${H(`submit`,t=>void e.prepareSend(t))}
      >
        <label
          >Recipient<input
            name="recipient"
            data-testid="recipient"
            placeholder="paydev1…"
            autocomplete="off"
            spellcheck="false"
            required
            ${U(`disabled`,e.busy)}
        /></label>
        <label
          >Amount<input
            name="amount"
            data-testid="amount"
            placeholder="0.00"
            inputmode="decimal"
            autocomplete="off"
            required
            ${U(`disabled`,e.busy)}
        /></label>
        <div class="amount-presets" aria-label="Amount shortcuts">
          ${lr(e,`25%`,4,`amount-quarter`)}
          ${lr(e,`50%`,2,`amount-half`)}
          ${lr(e,`Max`,1,`amount-max`)}
        </div>
        <div class="payment-summary">
          <span>Available</span
          ><strong
            >${()=>e.balanceHidden.value?`••••`:Y(e.account.value?.balance??`0`,e.status.value)}</strong
          >
          <span>Network fee</span
          ><strong
            >0.00 ${()=>e.status.value?.asset.symbol??`TEST`}</strong
          >
        </div>
        <button
          class="primary"
          data-testid="send-payment"
          type="submit"
          ${U(`disabled`,()=>e.busy.value||!e.account.value)}
        >
          Review payment
        </button>
      </form>
      ${()=>e.pendingTransfer.value?dr(e,e.pendingTransfer.value):null}
      ${()=>e.receipt.value?pr(e.receipt.value):null}
    </article>
  </div>`}function cr(e){return V`<ol class="payment-steps" aria-label="Payment progress">
    <li data-active="true"><b>1</b><span>Details</span></li>
    <li
      ${U(`data-active`,()=>String(!!(e.pendingTransfer.value||e.receipt.value)))}
    >
      <b>2</b><span>Verify</span>
    </li>
    <li ${U(`data-active`,()=>String(!!e.receipt.value))}>
      <b>3</b><span>Finality</span>
    </li>
  </ol>`}function lr(e,t,n,r){return V`<button
    type="button"
    ${U(`data-testid`,r)}
    ${U(`disabled`,()=>e.busy.value||!e.account.value)}
    ${H(`click`,t=>ur(t,e,n))}
  >
    ${t}
  </button>`}function ur(e,t,n){let r=e.currentTarget;if(!(r instanceof HTMLButtonElement))return;let i=r.form?.elements.namedItem(`amount`);!(i instanceof HTMLInputElement)||!t.status.value||(i.value=o((BigInt(t.account.value?.balance??`0`)/BigInt(n)).toString(),t.status.value.asset.decimals),i.focus())}function dr(e,t){return V`<section class="transfer-review" data-testid="transfer-review">
    <div class="review-heading">
      <p class="eyebrow">Confirm signed payment</p>
      <span>Not signed yet</span>
    </div>
    <dl>
      <div class="recipient-review">
        <dt>Recipient</dt>
        <dd>${t.recipient}</dd>
      </div>
      <div>
        <dt>Amount</dt>
        <dd>${Y(t.amount,e.status.value)}</dd>
      </div>
      <div>
        <dt>Balance after</dt>
        <dd>${fr(e,t.amount)}</dd>
      </div>
      <div>
        <dt>Network fee</dt>
        <dd>0 ${e.status.value?.asset.symbol??`TEST`}</dd>
      </div>
    </dl>
    <p>The private key stays on this device. Only this operation is signed.</p>
    <div class="button-row">
      <button class="secondary" type="button" ${H(`click`,e.cancelSend)}>
        Edit details</button
      ><button
        class="primary"
        data-testid="confirm-transfer"
        type="button"
        ${U(`disabled`,e.busy)}
        ${H(`click`,()=>void e.confirmSend())}
      >
        Sign & finalize
      </button>
    </div>
  </section>`}function fr(e,t){let n=BigInt(e.account.value?.balance??`0`),r=BigInt(t);return Y((n>=r?n-r:0n).toString(),e.status.value)}function pr(e){return V`<div class="receipt" data-testid="receipt">
    <i>✓</i
    ><span
      ><strong>Payment finalized</strong
      ><small>Block #${e.checkpoint.height}</small></span
    ><code>${Yn(e.transaction.id)}</code
    ><a
      ${U(`href`,`https://explorer.payrail.one/?tx=${e.transaction.id}`)}
      >View proof ↗</a
    >
  </div>`}function mr(e){return V`<div class="bridge-layout">
    <section class="wallet-heading bridge-heading">
      <div>
        <p class="eyebrow">Ethereum bridge · testnet</p>
        <h1>Bring tUSDT to Payrail.</h1>
        <p class="heading-note">
          Deposit test USDT from Sepolia. MetaMask signs on Ethereum; your
          Payrail key never leaves this browser.
        </p>
      </div>
      <span class="bridge-network"><i>Ξ</i>${Rt}</span>
    </section>

    <section class="bridge-route" aria-label="Bridge route">
      ${_r(`From`,`Ethereum Sepolia`,()=>e.bridge.ethereumAddress.value,()=>Sr(e.bridge.ethereumBalance.value,e.status.value?.bridgeAsset?.decimals??6,`tUSDT`))}
      <div class="bridge-arrow" aria-hidden="true">→</div>
      ${_r(`To`,`Payrail Devnet`,()=>e.wallet.value?.address??null,()=>Sr(e.bridge.payrailBalance.value?.balance??`0`,e.status.value?.bridgeAsset?.decimals??6,e.status.value?.bridgeAsset?.symbol??`TUSDT`))}
    </section>

    <div class="bridge-grid">
      <article class="controls bridge-action-panel">
        <div class="panel-title">
          <div>
            <p class="eyebrow">Deposit</p>
            <h2>Sepolia → Payrail</h2>
          </div>
          <span class="risk-pill">Inbound</span>
        </div>
        ${()=>e.bridge.ethereumAddress.value?gr(e):hr(e)}
      </article>
      <aside class="bridge-status-panel">
        <p class="eyebrow">Settlement status</p>
        <ol>
          ${vr(e,`approving`,`Approve tUSDT`)}
          ${vr(e,`depositing`,`Deposit to vault`)}
          ${vr(e,`waiting`,`Wait for Ethereum finality`)}
          ${vr(e,`complete`,`Credit Payrail balance`)}
        </ol>
        ${()=>br(e.bridge.transactionHash.value)}
        <dl>
          <div>
            <dt>Token</dt>
            <dd ${U(`title`,G)}>
              ${Cr(G)}
            </dd>
          </div>
          <div>
            <dt>Vault</dt>
            <dd ${U(`title`,zt)}>
              ${Cr(zt)}
            </dd>
          </div>
        </dl>
      </aside>
    </div>
    <p class="bridge-warning">
      tUSDT is an unbacked Sepolia test token, not Tether USD₮. This testnet
      screen supports deposits only; mainnet and withdrawals remain disabled.
    </p>
  </div>`}function hr(e){return V`<div class="bridge-connect">
    <div class="metamask-mark">Ξ</div>
    <h3>Connect MetaMask</h3>
    <p>
      The wallet will request the Sepolia network and two confirmations:
      approval, then deposit.
    </p>
    <button
      class="primary"
      data-testid="connect-ethereum"
      type="button"
      ${U(`disabled`,e.bridge.busy)}
      ${H(`click`,()=>void e.bridge.connect())}
    >
      ${()=>e.bridge.stage.value===`connecting`?`Connecting…`:e.bridge.providerAvailable?`Connect MetaMask`:`Install MetaMask`}
    </button>
  </div>`}function gr(e){return V`<form class="bridge-form" ${H(`submit`,e.bridge.deposit)}>
    <label for="bridge-amount">Amount</label>
    <div class="bridge-amount-input">
      <input
        id="bridge-amount"
        name="bridge-amount"
        data-testid="bridge-amount"
        inputmode="decimal"
        autocomplete="off"
        placeholder="1.00"
        required
      />
      <strong>tUSDT</strong>
    </div>
    <p>
      Destination
      <code ${U(`title`,()=>e.wallet.value?.address??``)}
        >${()=>Cr(e.wallet.value?.address??``)}</code
      >
    </p>
    <div class="button-row">
      <button
        class="primary"
        data-testid="submit-bridge-deposit"
        type="submit"
        ${U(`disabled`,e.bridge.busy)}
      >
        ${()=>xr(e.bridge.stage.value)}
      </button>
      <button
        class="secondary"
        type="button"
        ${U(`disabled`,e.bridge.busy)}
        ${H(`click`,()=>void e.bridge.refresh())}
      >
        Refresh
      </button>
    </div>
  </form>`}function _r(e,t,n,r){return V`<article>
    <span>${e}</span>
    <strong>${t}</strong>
    <code ${U(`title`,()=>n()??``)}
      >${()=>n()?Cr(n()??``):`Not connected`}</code
    >
    <b>${r}</b>
  </article>`}function vr(e,t,n){return V`<li
    ${U(`data-state`,()=>yr(e.bridge.stage.value,t))}
  >
    <i></i><span>${n}</span>
  </li>`}function yr(e,t){let n=[`idle`,`connecting`,`approving`,`depositing`,`waiting`,`complete`],r=n.indexOf(e),i=n.indexOf(t);return e===t?`active`:r>i?`done`:`idle`}function br(e){return e?V`<a
        class="bridge-transaction"
        ${U(`href`,`https://sepolia.etherscan.io/tx/${e}`)}
        target="_blank"
        rel="noreferrer"
        >View Ethereum transaction ↗</a
      >`:null}function xr(e){return e===`approving`?`Approving…`:e===`depositing`?`Depositing…`:e===`waiting`?`Waiting for finality…`:e===`complete`?`Deposit more tUSDT`:`Bridge to Payrail`}function Sr(e,t,n){let r=BigInt(e),i=10n**BigInt(t),a=r/i,o=(r%i).toString().padStart(t,`0`).replace(/0+$/,``);return`${a}${o?`.${o}`:``} ${n}`}function Cr(e){return e.length>18?`${e.slice(0,10)}…${e.slice(-6)}`:e}function wr(e){return V`<div class="settings-page">
    <section class="settings-heading">
      <div>
        <p class="eyebrow">Account control center</p>
        <h1>Private by default.<br /><em>Yours to configure.</em></h1>
        <p>
          Manage this device, live account updates and notification policy
          without exposing your signing key.
        </p>
      </div>
      <span class="settings-session"><i></i>Encrypted session</span>
    </section>
    <div class="settings-grid">
      ${Tr(e)} ${Er(e)} ${Dr(e)}
      ${Or(e)} ${kr(e)}
    </div>
  </div>`}function Tr(e){return V`<article class="settings-card identity-settings">
    <div class="settings-card-head">
      ${jr(`identity`)}
      <div>
        <p class="eyebrow">Profile</p>
        <h2>Wallet identity</h2>
      </div>
    </div>
    <label>Public account</label>
    <code>${()=>e.wallet.value?.address??`—`}</code>
    <dl>
      <div>
        <dt>Network</dt>
        <dd>Payrail Devnet</dd>
      </div>
      <div>
        <dt>Signing</dt>
        <dd>On-device Ed25519</dd>
      </div>
      <div>
        <dt>Asset</dt>
        <dd>${()=>e.status.value?.asset.symbol??`—`}</dd>
      </div>
    </dl>
    <button type="button" ${H(`click`,()=>void Pr(e))}>
      Copy full address
    </button>
  </article>`}function Er(e){return V`<article class="settings-card live-settings">
    <div class="settings-card-head">
      ${jr(`live`)}
      <div>
        <p class="eyebrow">Realtime</p>
        <h2>Live account stream</h2>
      </div>
      <span
        data-testid="live-status"
        class="connection-pill"
        ${U(`data-status`,e.liveStatus)}
        >${()=>Fr(e.liveStatus.value)}</span
      >
    </div>
    <p>
      Finality and checkout events arrive over a read-only, account-filtered
      WebSocket with heartbeat and automatic reconnect.
    </p>
    <dl>
      <div>
        <dt>Transport</dt>
        <dd>WSS · payrail.live.v1</dd>
      </div>
      <div>
        <dt>Heartbeat</dt>
        <dd>25 seconds</dd>
      </div>
      <div>
        <dt>Last event</dt>
        <dd>${()=>Lr(e.lastLiveEvent.value)}</dd>
      </div>
    </dl>
    <button type="button" ${H(`click`,e.reconnectLive)}>
      Reconnect stream
    </button>
  </article>`}function Dr(e){return V`<article class="settings-card notification-settings">
    <div class="settings-card-head">
      ${jr(`notification`)}
      <div>
        <p class="eyebrow">Notifications</p>
        <h2>Signal, not noise</h2>
      </div>
    </div>
    <div class="permission-row">
      <span
        ><strong>Browser permission</strong
        ><small
          >${()=>Ir(e.notificationPermission.value)}</small
        ></span
      >
      <button
        class="compact-button"
        type="button"
        ${U(`disabled`,()=>e.notificationPermission.value===`granted`||e.notificationPermission.value===`denied`||e.notificationPermission.value===`unsupported`)}
        ${H(`click`,()=>void e.requestNotifications())}
      >
        ${()=>e.notificationPermission.value===`granted`?`Enabled`:e.notificationPermission.value===`denied`?`Blocked`:`Enable`}
      </button>
    </div>
    <div class="preference-list">
      ${Ar(e,`browserNotifications`,`Desktop notifications`,`Show events outside this tab`)}
      ${Ar(e,`incomingPayments`,`Incoming payments`,`Alert when value reaches this account`)}
      ${Ar(e,`outgoingPayments`,`Outgoing finality`,`Confirm when a payment settles`)}
      ${Ar(e,`checkoutUpdates`,`Payment requests`,`Track request settlement live`)}
      ${Ar(e,`notificationSound`,`Sound`,`Play a subtle confirmation tone`)}
    </div>
  </article>`}function Or(e){return V`<article class="settings-card security-settings">
    <div class="settings-card-head">
      ${jr(`security`)}
      <div>
        <p class="eyebrow">Security</p>
        <h2>Device & session</h2>
      </div>
    </div>
    <label class="select-setting">
      <span
        ><strong>Automatic lock</strong
        ><small>Reset by trusted wallet actions</small></span
      >
      <select ${H(`change`,t=>Nr(e,t))}>
        ${Mr(e,5)} ${Mr(e,10)}
        ${Mr(e,30)}
      </select>
    </label>
    <div class="security-proof">
      <div>
        <i>✓</i
        ><span
          ><strong>Encrypted vault</strong
          ><small>AES-256-GCM · 600K PBKDF2 rounds</small></span
        >
      </div>
      <div>
        <i>✓</i
        ><span
          ><strong>Reload protection</strong
          ><small>Non-extractable signing key · timed session</small></span
        >
      </div>
      <div>
        <i>✓</i
        ><span
          ><strong>No cloud recovery</strong
          ><small>Payrail never receives your private key</small></span
        >
      </div>
    </div>
    <div class="settings-actions">
      <button type="button" ${H(`click`,e.backup)}>
        Download backup
      </button>
      <button
        class="danger-button"
        type="button"
        ${H(`click`,()=>void e.lock())}
      >
        Lock now
      </button>
    </div>
  </article>`}function kr(e){return V`<article class="settings-card recovery-settings">
    <div class="settings-card-head">
      ${jr(`recovery`)}
      <div>
        <p class="eyebrow">Recovery</p>
        <h2>Move to a new device</h2>
      </div>
    </div>
    <p>
      Your encrypted backup is the portable key to this account. It is safe to
      store offline, but it can only be opened with your wallet password.
    </p>
    <ol class="recovery-steps">
      <li>
        <b>1</b
        ><span
          ><strong>Download</strong
          ><small>Save the encrypted JSON backup</small></span
        >
      </li>
      <li>
        <b>2</b
        ><span
          ><strong>Transfer</strong
          ><small>Move it securely to the new device</small></span
        >
      </li>
      <li>
        <b>3</b
        ><span
          ><strong>Restore</strong
          ><small>Import it and enter the same password</small></span
        >
      </li>
    </ol>
    <div class="recovery-warning">
      <i>!</i
      ><span
        ><strong>No custodial reset</strong
        ><small
          >Without the backup or an unlocked device, Payrail cannot recover this
          self-custody wallet.</small
        ></span
      >
    </div>
    <button class="primary" type="button" ${H(`click`,e.backup)}>
      Download encrypted backup
    </button>
  </article>`}function Ar(e,t,n,r){return V`<label class="toggle-setting">
    <span><strong>${n}</strong><small>${r}</small></span>
    <input
      type="checkbox"
      ${U(`checked`,()=>!!e.preferences.value[t])}
      ${H(`change`,n=>e.updatePreference(t,n.currentTarget.checked))}
    />
    <i aria-hidden="true"></i>
  </label>`}function jr(e){return V`<span class="settings-icon">${X(e)}</span>`}function Mr(e,t){return V`<option
    ${U(`value`,t)}
    ${U(`selected`,()=>e.preferences.value.autoLockMinutes===t)}
  >
    ${t} minutes
  </option>`}function Nr(e,t){let n=Number(t.currentTarget.value);(n===5||n===10||n===30)&&e.updatePreference(`autoLockMinutes`,n)}async function Pr(e){await navigator.clipboard.writeText(e.wallet.value?.address??``)}function Fr(e){return e===`live`?`Live`:e===`connecting`?`Connecting`:e===`reconnecting`?`Reconnecting`:`Offline`}function Ir(e){return e===`unsupported`?`Not supported by this browser`:e===`granted`?`Granted on this device`:e===`denied`?`Blocked in browser settings`:`Permission not requested`}function Lr(e){return e===null?`Waiting for first event`:new Intl.DateTimeFormat(void 0,{hour:`2-digit`,minute:`2-digit`,second:`2-digit`}).format(e)}function Rr(e){return V`<section class="wallet-home">
    <div class="wallet-toolbar">
      <div class="wallet-tabs" role="navigation" aria-label="Wallet">
        ${Z(e,`overview`,`Overview`)}${Z(e,`send`,`Send`)}${Z(e,`request`,`Request`)}${Z(e,`bridge`,`Bridge`)}${Z(e,`code`,`Payrail Code`)}${Z(e,`settings`,`Settings`)}
      </div>
      <div class="session-actions">
        <button
          data-testid="backup-wallet"
          type="button"
          ${H(`click`,e.backup)}
        >
          Backup
        </button>
        <button
          data-testid="lock-wallet"
          type="button"
          ${H(`click`,()=>void e.lock())}
        >
          Lock
        </button>
      </div>
    </div>
    ${()=>zr(e)}
    <footer>
      <span>${e.message}</span
      ><span
        >Encrypted locally · auto-lock
        ${()=>e.preferences.value.autoLockMinutes} min</span
      >
    </footer>
  </section>`}function Z(e,t,n){return V`<button
    type="button"
    ${U(`data-testid`,`${t}-tab`)}
    ${U(`data-active`,()=>String(e.view.value===t))}
    ${H(`click`,()=>e.selectView(t))}
  >
    ${n}
  </button>`}function zr(e){switch(e.view.value){case`send`:return sr(e);case`request`:return nr(e);case`code`:return $n(e);case`bridge`:return mr(e);case`checkout`:return ar(e);case`settings`:return wr(e);default:return Br(e)}}function Br(e){return V`<div class="wallet-dashboard">
    <section class="wallet-heading">
      <div>
        <p class="eyebrow">Payrail private wallet</p>
        <h1>Money in motion.</h1>
        <p class="heading-note">
          A self-custody account with bank-grade clarity and verifiable
          settlement.
        </p>
      </div>
      <span class="finality-badge"
        ><i></i>Finalized
        #${()=>e.status.value?.finalizedHeight??`—`}</span
      >
    </section>
    <div class="wallet-grid">
      ${Qn(e)}
      <article class="controls account-panel">
        <div class="panel-title">
          <div>
            <p class="eyebrow">Account</p>
            <h2>Self-custody controls</h2>
          </div>
          <span class="risk-pill">Protected</span>
        </div>
        <div class="address-row">
          <div class="address-identity">
            <span>Public address</span>
            <code
              data-testid="wallet-address"
              ${U(`data-address`,()=>e.wallet.value?.address??``)}
              ${U(`title`,()=>e.wallet.value?.address??``)}
              >${()=>e.wallet.value?.address??``}</code
            >
          </div>
          <button
            type="button"
            ${H(`click`,()=>void navigator.clipboard.writeText(e.wallet.value?.address??``))}
          >
            Copy full
          </button>
        </div>
        <dl class="account-facts">
          <div>
            <dt>Nonce</dt>
            <dd>${()=>e.account.value?.nonce??`—`}</dd>
          </div>
          <div>
            <dt>Network</dt>
            <dd ${U(`title`,()=>e.status.value?.networkId??``)}>
              Payrail Devnet
            </dd>
          </div>
          <div>
            <dt>Signing</dt>
            <dd>On-device</dd>
          </div>
        </dl>
        <div class="button-row">
          <button
            class="primary"
            data-testid="fund-wallet"
            type="button"
            ${U(`disabled`,e.busy)}
            ${H(`click`,()=>void e.fund())}
          >
            Add test funds</button
          ><button
            class="secondary"
            type="button"
            ${U(`disabled`,e.busy)}
            ${H(`click`,()=>void e.refresh())}
          >
            Refresh state
          </button>
        </div>
      </article>
    </div>
    ${Vr(e)}
    <section class="quick-actions">
      <button
        data-testid="payrail-code-action"
        type="button"
        ${H(`click`,()=>e.selectView(`code`))}
      >
        <i>6</i
        ><span
          ><strong>Payrail Code</strong
          ><small>Six digits · two minutes</small></span
        >
      </button>
      <button type="button" ${H(`click`,()=>e.selectView(`send`))}>
        <i>${X(`send`)}</i
        ><span
          ><strong>Send payment</strong><small>Sign and finalize</small></span
        >
      </button>
      <button type="button" ${H(`click`,()=>e.selectView(`request`))}>
        <i>${X(`request`)}</i
        ><span
          ><strong>Request money</strong><small>Web, QR and SMS</small></span
        >
      </button>
      <button type="button" ${H(`click`,()=>e.selectView(`bridge`))}>
        <i>Ξ</i
        ><span
          ><strong>Bridge tUSDT</strong><small>Sepolia → Payrail</small></span
        >
      </button>
      <a href="https://explorer.payrail.one"
        ><i>${X(`explorer`)}</i
        ><span
          ><strong>Open Explorer</strong
          ><small>Independent finality</small></span
        ></a
      >
    </section>
    <div class="dashboard-lower">
      ${Hr(e)} ${Ur(e)}
    </div>
  </div>`}function Vr(e){return V`<section
    class="portfolio-metrics"
    aria-label="Recent account insights"
  >
    <div>
      <span>Recent inflow</span
      ><strong
        >${()=>e.balanceHidden.value?`••••`:Y(Gr(e).incoming,e.status.value)}</strong
      ><small>Indexed activity window</small>
    </div>
    <div>
      <span>Recent outflow</span
      ><strong
        >${()=>e.balanceHidden.value?`••••`:Y(Gr(e).outgoing,e.status.value)}</strong
      ><small>Indexed activity window</small>
    </div>
    <div>
      <span>Finalized payments</span
      ><strong
        >${()=>Gr(e).count.toString().padStart(2,`0`)}</strong
      ><small>For this account</small>
    </div>
    <div>
      <span>Settlement</span><strong>Deterministic</strong
      ><small>${()=>Xn(e.status.value)}</small>
    </div>
  </section>`}function Hr(e){return V`<article class="activity-panel">
    <div class="panel-title">
      <div>
        <p class="eyebrow">Ledger activity</p>
        <h2>Recent payments</h2>
      </div>
      <a href="https://explorer.payrail.one">View explorer ↗</a>
    </div>
    <div class="activity-head">
      <span>Payment</span><span>Direction</span><span>Block</span
      ><span>Amount</span>
    </div>
    ${$e(()=>Wr(e),e=>e.id,t=>{let n=()=>t.value.to===e.wallet.value?.address;return V`<a
          class="activity-row"
          ${U(`href`,()=>`https://explorer.payrail.one/?tx=${t.value.id}`)}
          ><span class="payment-id"
            ><i>${()=>n()?`↓`:`↗`}</i
            ><code>${()=>Yn(t.value.id)}</code></span
          ><span ${U(`data-direction`,()=>n()?`in`:`out`)}
            >${()=>n()?`Received`:`Sent`}</span
          ><span>#${()=>t.value.blockHeight}</span
          ><strong
            >${()=>`${n()?`+`:`−`}${Y(t.value.amount,e.status.value)}`}</strong
          ></a
        >`})}
    ${()=>Wr(e).length?null:V`<div class="empty-state">
            <i>${X(`send`)}</i
            ><strong>Your first payment will appear here</strong
            ><span>Every row links to independent finality evidence.</span>
          </div>`}
  </article>`}function Ur(e){return V`<aside class="assurance-panel">
    <div class="assurance-head">
      <span class="live-dot"><i></i>Network live</span
      ><small>Trust, made visible</small>
    </div>
    <h2>Settlement you can prove.</h2>
    <p>
      Payrail separates the private signing boundary from public, independently
      indexed finality.
    </p>
    <dl>
      <div>
        <dt>Latest finalized block</dt>
        <dd>#${()=>e.status.value?.finalizedHeight??`—`}</dd>
      </div>
      <div>
        <dt>Consensus</dt>
        <dd>${()=>Xn(e.status.value)}</dd>
      </div>
      <div>
        <dt>Private key</dt>
        <dd>On this device</dd>
      </div>
    </dl>
    <a href="https://explorer.payrail.one"
      >Inspect the network <span>↗</span></a
    >
  </aside>`}function Wr(e){let t=e.wallet.value?.address;return(e.overview.value?.transactions??[]).filter(e=>e.from===t||e.to===t).slice(0,6)}function Gr(e){let t=e.wallet.value?.address,n=0n,r=0n,i=Wr(e);for(let e of i){let i=BigInt(e.amount);e.to===t&&(n+=i),e.from===t&&(r+=i)}return{incoming:n.toString(),outgoing:r.toString(),count:i.length}}function Kr(e,t,n){let r,i,a,o=0,s=!1,c=()=>{i!==void 0&&window.clearTimeout(i),a!==void 0&&window.clearInterval(a),i=void 0,a=void 0},l=()=>{if(s||i!==void 0)return;t(navigator.onLine?`reconnecting`:`offline`);let e=Math.min(3e4,1e3*2**Math.min(o,5));o+=1,i=window.setTimeout(()=>{i=void 0,u()},e)},u=()=>{if(s||!navigator.onLine){t(`offline`);return}t(o===0?`connecting`:`reconnecting`);let i=new WebSocket(Tt(e),`payrail.live.v1`);r=i,i.addEventListener(`open`,()=>{r===i&&(o=0,t(`live`),a=window.setInterval(()=>{i.readyState===WebSocket.OPEN&&i.send(`ping`)},25e3))}),i.addEventListener(`message`,e=>{let t=qr(e.data);t&&n(t)}),i.addEventListener(`close`,()=>{r===i&&(r=void 0,a!==void 0&&window.clearInterval(a),a=void 0,l())}),i.addEventListener(`error`,()=>i.close())},d=()=>{c(),o=0;let e=r;r=void 0,e?.close(1e3,`client reconnect`),u()},f=()=>d(),p=()=>t(`offline`);return window.addEventListener(`online`,f),window.addEventListener(`offline`,p),u(),{reconnect:d,close(){s=!0,c(),window.removeEventListener(`online`,f),window.removeEventListener(`offline`,p),r?.close(1e3,`wallet locked`),r=void 0,t(`offline`)}}}function qr(e){if(typeof e!=`string`)return null;let t;try{t=JSON.parse(e)}catch{return null}return!Jr(t)||typeof t.type!=`string`?null:t.type===`resyncRequired`?{type:`resyncRequired`}:t.type===`finalized`&&Jr(t.transaction)&&Jr(t.checkpoint)||t.type===`checkoutUpdated`&&Jr(t.checkout)?t:null}function Jr(e){return typeof e==`object`&&!!e&&!Array.isArray(e)}var Yr=`payrail.wallet.preferences.v1`,Xr={autoLockMinutes:10,browserNotifications:!1,incomingPayments:!0,outgoingPayments:!0,checkoutUpdates:!0,notificationSound:!1};function Zr(){try{let e=JSON.parse(localStorage.getItem(Yr)??`null`);return ei(e)?{autoLockMinutes:$r(e.autoLockMinutes)?e.autoLockMinutes:Xr.autoLockMinutes,browserNotifications:e.browserNotifications===!0,incomingPayments:e.incomingPayments!==!1,outgoingPayments:e.outgoingPayments!==!1,checkoutUpdates:e.checkoutUpdates!==!1,notificationSound:e.notificationSound===!0}:Xr}catch{return Xr}}function Qr(e){localStorage.setItem(Yr,JSON.stringify(e))}function $r(e){return e===5||e===10||e===30}function ei(e){return typeof e==`object`&&!!e&&!Array.isArray(e)}function ti(e,t,n){let r,i;if(e.type===`finalized`){let a=e.transaction.to===t;if(a&&!n.incomingPayments||!a&&!n.outgoingPayments)return;r=a?`Payment received`:`Payment finalized`,i=`Finalized in block #${e.checkpoint.height}.`}else if(e.type===`checkoutUpdated`){if(!n.checkoutUpdates)return;r=`Payment request updated`,i=`Request ${e.checkout.status}.`}else return;n.browserNotifications&&`Notification`in window&&Notification.permission===`granted`&&new Notification(r,{body:i,tag:`payrail-wallet-event`}),n.notificationSound&&ni()}function ni(){try{let e=new AudioContext,t=e.createOscillator(),n=e.createGain();t.frequency.value=660,n.gain.setValueAtTime(.04,e.currentTime),n.gain.exponentialRampToValueAtTime(1e-4,e.currentTime+.18),t.connect(n).connect(e.destination),t.start(),t.stop(e.currentTime+.18),t.addEventListener(`ended`,()=>void e.close())}catch{}}function ri(){return V`<div class="shell privacy-shell">
    <header class="pr-public-header">
      ${Ct()}
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
  </div>`}var ii=`https://lottery.payrail.one`;function ai(e){let t=new URLSearchParams(e.replace(/^#/u,``));if(t.get(`mode`)!==`phone-link`)return null;let n=t.get(`challenge`)??``,r=t.get(`token`)??``,i=t.get(`origin`)??``;if(i!==ii||!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u.test(n)||!/^[A-Za-z0-9_-]{43}$/u.test(r))throw Error(`The phone-link request is malformed or untrusted.`);return{challengeId:n,token:r,origin:i}}async function oi(e){let t=await li(await fetch(`${e.origin}/api/wallet-links/challenges/${e.challengeId}`,{headers:{authorization:`Bearer ${e.token}`}}));if(typeof t.id!=`string`||typeof t.message!=`string`||typeof t.expiresAt!=`number`||typeof t.perEntryLimitMinor!=`string`||typeof t.dailyLimitMinor!=`string`||typeof t.mandateExpiresAt!=`number`)throw Error(`The phone-link response is invalid.`);let n=ye(t.message);if(t.id!==e.challengeId||t.expiresAt!==n.expiresAt||t.perEntryLimitMinor!==n.perEntryLimitMinor.toString()||t.dailyLimitMinor!==n.dailyLimitMinor.toString()||t.mandateExpiresAt!==n.mandateExpiresAt)throw Error(`The phone-link terms do not match the signed message.`);return{...t,terms:n}}async function si(e,t,r){ye(t.message);let i=await r.signPhoneLinkChallenge(t.message),a=await li(await fetch(`${e.origin}/api/wallet-links/challenges/${e.challengeId}/complete`,{method:`POST`,headers:{authorization:`Bearer ${e.token}`,"content-type":`application/json`},body:JSON.stringify({walletAddress:r.address,walletAccountId:r.accountId,publicKeyHex:n(r.publicKey),signatureHex:i})}));if(typeof a.returnUrl!=`string`)throw Error(`The phone-link completion response is invalid.`);let o=new URL(a.returnUrl);if(o.origin!==e.origin)throw Error(`The phone-link return URL is untrusted.`);return o.toString()}function ci(e){return V`<section class="phone-link-layout">
    <article class="phone-link-card">
      <p class="eyebrow">Payrail verified identity</p>
      <h1>Link phone.<br /><em>Keep custody.</em></h1>
      <p class="phone-link-summary">
        Random One will recognize SMS purchases from this phone as this Payrail
        account. Your private key never leaves this wallet.
      </p>
      <dl>
        <div>
          <dt>Wallet</dt>
          <dd>${()=>fi(e.wallet.value?.address??``)}</dd>
        </div>
        <div>
          <dt>Per ticket</dt>
          <dd>${()=>ui(e.challenge.value?.perEntryLimitMinor)} PR</dd>
        </div>
        <div>
          <dt>24-hour limit</dt>
          <dd>${()=>ui(e.challenge.value?.dailyLimitMinor)} PR</dd>
        </div>
        <div>
          <dt>Authorization ends</dt>
          <dd>${()=>di(e.challenge.value?.mandateExpiresAt)}</dd>
        </div>
      </dl>
      <div class="phone-link-warning">
        This demo authorization identifies the wallet and limits test-credit SMS
        purchases. It does not expose a key or permit arbitrary transfers.
      </div>
      <div class="phone-link-actions">
        <button type="button" class="secondary" ${H(`click`,e.cancel)}>
          Cancel
        </button>
        <button
          type="button"
          class="primary"
          ${U(`disabled`,()=>e.busy.value||!e.challenge.value)}
          ${H(`click`,()=>void e.approve())}
        >
          Review and sign
        </button>
      </div>
      <p class="status" role="status">${e.message}</p>
    </article>
  </section>`}async function li(e){let t=await e.json().catch(()=>null);if(!e.ok){let n=t&&typeof t==`object`&&`error`in t?String(t.error):`Request failed (${e.status}).`;throw Error(n)}if(!t||typeof t!=`object`||Array.isArray(t))throw Error(`Service returned an invalid response.`);return t}function ui(e){if(!e)return`—`;let t=BigInt(e);return`${t/100n}.${(t%100n).toString().padStart(2,`0`)}`}function di(e){return e?new Date(e).toLocaleDateString():`—`}function fi(e){return e?`${e.slice(0,12)}…${e.slice(-8)}`:`—`}function pi(){let e=R(null),t=R(null),n=R(null),i=R(null),o=R(null),s=R(null),c=R(null),l=R(null),u=R(!1),d=R(null),f=R(0),p=gi(location.pathname),m=R(p?`checkout`:`overview`),h=R(`Connecting to Payrail testnet…`),ee=R(null),g=null;try{g=ai(location.hash)}catch(e){h.value=$(e)}let _=R(!1),v=R(!1),y=R(Zr()),te=R(`offline`),ne=R(null),re=R(`Notification`in window?Notification.permission:`unsupported`),ie=sn({wallet:n,status:e,setMessage:e=>h.value=e}),b,x,S,C,w=()=>{S!==void 0&&window.clearTimeout(S),C!==void 0&&window.clearInterval(C),S=void 0,C=void 0},ae=()=>{w(),d.value=null,f.value=0,h.value=`Payrail Code cancelled. No payment was authorized.`};async function oe(e){if(d.value?.sessionToken===e)try{let t=await W.approvalCodeChallenge(e);if(d.value?.sessionToken!==e)return;if(t.status!==`waiting`&&t.checkout){w(),d.value=null,f.value=0,l.value=t.checkout,u.value=!1,m.value=`checkout`,T(),h.value=`Payrail Code linked. Review the merchant and amount before signing.`;return}S=window.setTimeout(()=>void oe(e),1500)}catch(t){if(d.value?.sessionToken!==e)return;w(),d.value=null,f.value=0,h.value=$(t)}}async function se(){!n.value||!e.value||await D(async()=>{w();let t=await n.value?.createApprovalCodeIssue({networkId:e.value?.networkId??``,deviceId:hi()});if(!t)throw Error(`Wallet was locked before code issuance.`);let r=await W.issueApprovalCode(t);d.value=r;let i=()=>{let e=Math.max(0,Math.ceil((Number(r.expiresAtMs)-Date.now())/1e3));f.value=e,e===0&&d.value?.sessionToken===r.sessionToken&&(w(),d.value=null,h.value=`Payrail Code expired. Generate a new code.`)};i(),C=window.setInterval(i,1e3),T(),h.value=`Payrail Code ready. Enter it at checkout.`,oe(r.sessionToken)})}let T=()=>{if(b!==void 0&&window.clearTimeout(b),n.value){let e=y.value.autoLockMinutes*60*1e3;b=window.setTimeout(O,e),Nt(n.value.createSession(Date.now()+e)).catch(()=>void 0)}},E=async()=>{let[t,r]=await Promise.all([W.status(),W.overview()]);e.value=t,c.value=r,n.value&&(i.value=await W.account(n.value.address))},ce=e=>{x?.close(),x=Kr(e.address,e=>te.value=e,t=>{ne.value=Date.now(),t.type===`checkoutUpdated`&&(l.value=t.checkout),E().catch(e=>h.value=$(e)),ti(t,e.address,y.value)})},D=async e=>{_.value=!0;try{await e()}catch(e){h.value=$(e)}finally{_.value=!1}},le=async r=>{if(r.preventDefault(),!e.value){h.value=`Still connecting to Payrail. Try again in a moment.`;return}let i=new FormData(r.currentTarget),a=Q(i,`password`);if(a!==Q(i,`confirmation`)){h.value=`Passwords do not match.`;return}await D(async()=>{let r=await be(e.value?.addressPrefix??``,a);await jt(r.vault),t.value=r.vault,n.value=r.wallet,await E(),T(),ce(r.wallet),h.value=`Wallet created and encrypted on this device.`})},ue=async r=>{if(r.preventDefault(),!e.value||!t.value){h.value=`Still connecting to Payrail. Try again in a moment.`;return}let i=Q(new FormData(r.currentTarget),`password`);await D(async()=>{n.value=await Se(e.value?.addressPrefix??``,i,t.value),await E(),T(),ce(n.value),h.value=`Wallet unlocked locally.`})};async function O(){let e=n.value;b!==void 0&&window.clearTimeout(b),b=void 0,x?.close(),x=void 0,w(),d.value=null,f.value=0;try{await Pt()}catch(t){h.value=`Unable to securely lock this session. ${$(t)}`,T(),e&&ce(e);return}n.value=null,i.value=null,o.value=null,s.value=null,h.value=`Wallet locked. The encrypted vault remains on this device.`}let de=async()=>{n.value&&await D(async()=>{o.value=await W.faucet(n.value?.address??``),await E(),T(),h.value=`Test funds finalized on Payrail.`})},fe=async t=>{if(t.preventDefault(),!n.value||!i.value||!e.value)return;let r=new FormData(t.currentTarget);await D(async()=>{let t=Q(r,`recipient`),n=a(Q(r,`amount`),e.value?.asset.decimals??0);if(n>BigInt(i.value?.balance??`0`))throw Error(`Amount exceeds the finalized balance.`);ke(t,e.value?.addressPrefix??``),s.value={recipient:t,amount:n.toString()},o.value=null,h.value=`Review the exact recipient and amount before signing.`})},pe=async()=>{if(!n.value||!i.value||!e.value||!s.value)return;let t=s.value;await D(async()=>{let r=BigInt(t.amount);if(r>BigInt(i.value?.balance??`0`))throw Error(`Amount exceeds the finalized balance.`);let a=await n.value?.signTransfer({networkId:e.value?.networkId??``,assetId:e.value?.asset.id??``,idempotencyKey:crypto.getRandomValues(new Uint8Array(32)),recipientAccountId:ke(t.recipient,e.value?.addressPrefix??``),amount:r,fee:0n,nonce:BigInt(i.value?.nonce??`0`),validUntilHeight:BigInt(e.value?.finalizedHeight??`0`)+20n});if(!a)throw Error(`Wallet was locked before signing.`);o.value=await W.submit(a),await E(),T(),h.value=`Finalized in block #${o.value.checkpoint.height}.`,s.value=null})},me=()=>{s.value=null,h.value=`Payment cancelled before signing.`},he=async t=>{if(t.preventDefault(),!n.value||!e.value)return;let r=new FormData(t.currentTarget);await D(async()=>{let t=a(Q(r,`checkout-amount`),e.value?.asset.decimals??0);l.value=await W.createCheckout({merchantAddress:n.value?.address??``,amount:t.toString(),orderReference:Q(r,`order-reference`)}),T(),h.value=`One payment request is ready for web, QR and SMS.`})},ge=async()=>{if(!l.value)return;let e=l.value.smsText.replace(`{origin}`,`https://wallet.payrail.one`);try{await navigator.clipboard.writeText(e),h.value=`SMS payment request copied.`}catch{h.value=e}},_e=()=>{u.value=!0,h.value=`Review the exact recipient and amount before signing.`},ye=()=>{u.value=!1,h.value=`Payment cancelled before signing.`},Ce=async()=>{if(!n.value||!i.value||!e.value||!l.value)return;let t=l.value;await D(async()=>{let a=await n.value?.signTransfer({networkId:e.value?.networkId??``,assetId:t.asset.id,idempotencyKey:r(t.id,32),recipientAccountId:ke(t.merchantAddress,e.value?.addressPrefix??``),amount:BigInt(t.amount),fee:BigInt(t.fee),nonce:BigInt(i.value?.nonce??`0`),validUntilHeight:BigInt(t.validUntilHeight)});if(!a)throw Error(`Wallet was locked before signing.`);l.value=await W.submitCheckout(t.id,a),u.value=!1,await E(),T(),h.value=`Checkout finalized in block #${l.value.transaction?.blockHeight??`—`}.`})},k=()=>{if(!t.value)return;let e=new Blob([JSON.stringify(t.value)],{type:`application/json`}),n=document.createElement(`a`);n.href=URL.createObjectURL(e),n.download=`payrail-wallet-${t.value.address.slice(0,14)}.json`,n.click(),URL.revokeObjectURL(n.href),h.value=`Encrypted wallet backup downloaded.`},we=async r=>{let a=r.currentTarget,c=a.files?.[0];a.value=``,!(!c||!e.value)&&await D(async()=>{if(c.size>32768)throw Error(`Wallet backup is too large.`);let r=ve(JSON.parse(await c.text()));ke(r.address,e.value?.addressPrefix??``),await jt(r),t.value=r,n.value=null,i.value=null,o.value=null,s.value=null,h.value=`Encrypted wallet backup imported. Unlock it locally.`})},Te=async()=>{!g||!ee.value||!n.value||await D(async()=>{let e=await si(g,ee.value,n.value);T(),window.location.assign(e)})},A=()=>{g&&window.location.assign(`${g.origin}/#account`)};return g&&oi(g).then(e=>{ee.value=e,h.value=`Review the limits before signing this phone link.`}).catch(e=>h.value=$(e)),Promise.all([At(),Mt(),W.status(),W.overview(),p?W.checkout(p):null]).then(async([r,a,o,s,u])=>{if(t.value=r,e.value=o,c.value=s,l.value=u,r&&a)try{let e=await xe(o.addressPrefix,a);if(e.address!==r.address)throw Error(`Wallet session belongs to another vault.`);n.value=e,i.value=await W.account(e.address),T(),ce(e),h.value=`Secure session restored on this device.`;return}catch{await Pt()}h.value=r?`Payrail is live. Unlock your wallet.`:`Payrail is live. Create your encrypted wallet.`}).catch(e=>h.value=$(e)),V`<div class="shell">
    <header class="pr-public-header">
      ${Ct()}
      <nav>
        <a href="/privacy">Privacy</a>
        <a href="https://explorer.payrail.one">Explorer</a>
        <span class="network"><i></i>Payrail testnet</span>
      </nav>
    </header>
    <main>
      ${()=>n.value?g?ci({request:g,challenge:ee,wallet:n,busy:_,message:h,approve:Te,cancel:A}):Rr({status:e,wallet:n,account:i,receipt:o,pendingTransfer:s,overview:c,checkout:l,checkoutReviewed:u,approvalCode:d,codeSeconds:f,view:m,message:h,busy:_,balanceHidden:v,preferences:y,liveStatus:te,lastLiveEvent:ne,notificationPermission:re,toggleBalancePrivacy:()=>v.value=!v.value,updatePreference:(e,t)=>{y.value={...y.value,[e]:t},Qr(y.value),e===`autoLockMinutes`&&T()},requestNotifications:async()=>{`Notification`in window&&(re.value=await Notification.requestPermission(),re.value===`granted`&&(y.value={...y.value,browserNotifications:!0},Qr(y.value)))},reconnectLive:()=>x?.reconnect(),selectView:e=>{m.value=e,e===`bridge`&&ie.refresh().catch(e=>h.value=$(e))},bridge:ie,refresh:()=>D(E),fund:de,prepareSend:fe,confirmSend:pe,cancelSend:me,createCheckout:he,copySms:ge,reviewCheckout:_e,cancelCheckout:ye,confirmCheckout:Ce,createPayrailCode:se,cancelPayrailCode:ae,lock:O,backup:k}):It({vault:t,status:e,message:h,busy:_,createWallet:le,unlock:ue,importVault:we})}
    </main>
  </div>`}var mi=`payrail.code.device.v1`;function hi(){let e=localStorage.getItem(mi);if(e)try{return r(e,32)}catch{localStorage.removeItem(mi)}let t=crypto.getRandomValues(new Uint8Array(32));return localStorage.setItem(mi,n(t)),t}function gi(e){return/^\/pay\/([0-9a-f]{64})$/.exec(e)?.[1]??null}function Q(e,t){let n=e.get(t);if(typeof n!=`string`||n===``)throw Error(`${t} is required.`);return n}function $(e){return e instanceof Error?e.message:`Something went wrong.`}var _i=document.querySelector(`#app`);if(!_i)throw Error(`Missing application mount point.`);St(_i,location.pathname===`/privacy`?ri():pi());