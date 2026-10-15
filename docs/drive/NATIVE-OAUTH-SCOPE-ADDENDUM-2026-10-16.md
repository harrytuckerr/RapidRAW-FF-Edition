# Desktop OAuth scope clarification

Recorded 2026-10-16 (Australia/Melbourne) by the lead, independently checking the primary provider documentation. Applies to the desktop authorization interpretation of GD0, originally frozen at contract SHA-256 `c2784483077379393c4ca4d913972ccc125e93897832568bed9e8882f5c7ca19`. It changes no approved product scope and establishes no live-provider acceptance.

Google's installed-app OAuth documentation states that incremental authorization is unsupported. Its authorization request takes an explicit space-delimited scope set. See [Identify access scopes and authorization parameters](https://developers.google.com/identity/protocols/oauth2/native-app).

For the native desktop flow:

- State/library sync alone requests exactly `https://www.googleapis.com/auth/drive.appdata`.
- Photos alone requests exactly `https://www.googleapis.com/auth/drive.file`.
- Enabling both capabilities requests both approved scopes in a fresh authorization transaction. Do not use `include_granted_scopes` or assume an earlier state-only or photo-only grant accumulates into the combined grant.
- Requested scopes are not proof of granted access. Later token/provider integration must validate actual usable grants and verified account identity before enabling either capability or resuming its jobs. Failed, denied or cancelled authorization must preserve local edits and retain existing usable state without falsely reporting newly authorized access.
- Category enrolment remains explicit and separate from OAuth permissions. Authorization for both scopes does not automatically enrol photos, presets or profile libraries. No broad `drive` or `drive.readonly` fallback is permitted.

The contract's wording about separately connecting photos describes the user's capability choice, not a promise of installed-app incremental consent. The fresh-grant matrix requires independently established appdata-only, file-only and combined grant cases; sequential requests on one previously authorized session do not prove isolation or grant accumulation. Reconnect, declined additional access and preservation of an existing state-only connection require live tests before acceptance.

Android continues to use its separately specified GIS `AuthorizationClient` flow. This desktop clarification makes no claim about Android SDK grant-combination behavior; verify that platform's current API contract and actual grants independently. Cross-client permissions, Picker access, app-data visibility and distribution eligibility remain open gates.

## Evidence boundary

Pass: primary documentation checked independently; desktop scope-request contract clarified. GD1b1 implements only a native transaction helper and must have its own source review and execution receipt. Not exercised: Google login, consent, token exchange, renewal, secure storage, Android authorization, account switching, provider access and cross-device sync. No account, client registration, app binary or device state changed for this clarification.
