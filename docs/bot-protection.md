# Bot protection and consent controls

Glass does not hide browser automation. It does not spoof fingerprints, solve
CAPTCHAs, evade bot checks, or rotate identities.

Use a site-approved access path.

## If you operate the site

Register Glass as an allowed client. Possible controls include:

- verified bot registration;
- IP allowlisting for a known CI or staging range;
- an application token checked by site middleware; and
- a documented API.

Use the site API when it provides the required operation. An API has a versioned
contract and does not depend on page layout.

## Use an authenticated profile

A site may allow an authenticated session where it blocks an unauthenticated
browser. Use a persistent profile only when your deployment policy permits it:

```console
glass profiles create work
glass --headed --profile work navigate https://example.com/login
glass --profile work navigate https://example.com/dashboard
```

Profiles contain cookies and storage. Protect them.

Glass does not automate multi-factor authentication as a bypass. If the site
requires a new verification step, stop and complete the approved flow.

## Consent controls

Glass has bounded helpers for recognized OneTrust and Cookiebot controls.

| Framework | Control |
|---|---|
| OneTrust | A recognized visible consent button. |
| Cookiebot | A recognized visible consent button. |

The helper returns `dismissed`, `no_consent_found`, or
`unrecognized_framework`. It does not click generic text matches.

A consent helper is a page interaction. It is not a bot-protection bypass.

## Polite policy

Use the optional `polite` policy for public pages:

```console
glass --policy polite navigate https://example.com/public-page
```

The policy checks `robots.txt`, applies a bounded delay, and identifies
requests with the Glass user agent. A robots error or disallowed path fails
closed.

The policy does not bypass bot protection.

## Unsupported actions

Glass does not provide:

- fingerprint spoofing;
- CAPTCHA solving;
- anti-bot evasion;
- `navigator.webdriver` removal;
- user-agent rotation; or
- proxy identity rotation.

If a site blocks the supported access path, contact the site operator or use its
documented API.
