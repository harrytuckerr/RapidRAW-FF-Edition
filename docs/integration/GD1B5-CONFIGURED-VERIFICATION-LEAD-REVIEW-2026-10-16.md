# C2 lead source review

16 October 2026; gpt-6.1-sol. Reviewed Astra's source-only delivery based on
`0131d650706ee1a227f168015c07778d08623cb8`. No blocking static finding. This is
permission to perform separately bounded independent validation, not acceptance
of compilation, runtime, provider compatibility or canonical integration.

The lead read the full delivery receipt, production diff, changed native flow,
all new test definitions in the final source and relevant unchanged cleanup
functions. The private `configured_tests.rs` artifact is an earlier test-writing
scaffold; final test definitions are those in frozen session.rs, not that file.
The final source has twelve new definitions, including the foreign/late result
case and both admission/token-limited transport deadline cases.

The lead rehashed all twenty private manifest records, both source/receipt
deliveries and all 654 original input paths. Only session.rs changed; the exact
655-path inventory adds the one receipt. The frozen unified patch was applied
independently in memory to the entire baseline and reproduced all 112,397 final
bytes. Different Git/difflib hunk layout does not imply a content difference.

Source SHA-256: `1b77db23a04d51eddb9b1daff104e447dc0bbe6e803a611eb25a9f34fd7020fe`.
Receipt SHA-256: `767e670dea13d95473ca5123680a0a7c444d06607eb38e19ea48aaff678a65a2`.
Private manifest SHA-256: `898c419b03f1d69ca638a1c89e94f6a07ea6b387c95fe1d754d513d3b379c8ab`.

Static checks support the following conclusions:

- Immutable catalog resolution supplies native registration fields; empty,
  unknown and disallowed selections cannot start a worker. Raw starts preserve
  the pending-only continuation on either controller constructor.
- One mutex and the existing supervisor own the configured path. Extraction
  records authorization once and begins a separate verification state. The
  verification future stays local, uses the original deadline and cooperatively
  observes stop; no lock spans an await.
- Completion rebuilds current retained binding and checks the sealed candidate,
  current attempt/generation, verification phase, token and admission lifetime.
  The whole candidate is dropped before VerifiedNotAdopted publication. No
  connected credential owner, account snapshot or provider lease is introduced.
- Cancel, close, Drop, expiry and fault retain the authorization terminal and
  separately terminate verification once if started. Reaping remains behind the
  blocking worker and browser-ticket acknowledgment fence. Existing authored
  test region, permanent child and critical cleanup functions are byte-preserved.

The new tests use real restricted account HTTP verification but synthetic
post-exchange headers/reducer admission. Manual reducer reaping launches no
listener. Those cases cannot prove the browser/code/exchange/account chain,
actual provider identity or cleanup of resources they did not start. Independent
validation must use its own bounded fixtures, assert both terminal channels and
stale/cancel/deadline behavior, and retain existing public listener regressions.

Root prepared a separate review snapshot with the exact entire author prefix
and only a test-only child-module footer. The independent validator may add that
child and a new report; all pre-existing review inputs are immutable. Compile,
tests, lint, sockets, provider, app/Android, connected adoption and sync are
Not exercised by this source review. Canonical remains clean at 0131d650.
