# C-API friction

LibOSDP 4.0 is unreleased, so where its C API stands in the way of a
correct Rust API, the C API is what changes. This file records each case
met while porting libosdp-rs to 4.0: what the Rust side ran into, what
caused it, what was decided, and where it stands.

Status is one of: resolved upstream, accepted (the C behaviour is
intended and Rust follows it), or open (an upstream change is proposed
but not landed).

## Engine-owned commands completed re-entrantly with ACCEPTED

- **Symptom.** For `FILE_TX`, `PIVDATA`, `GENAUTH` and `CRAUTH` the
  completion callback fired inside `osdp_cp_submit_command()`, before it
  returned. Rust could hold no borrow across the call, and a submission
  "completed" the moment it started, which told the caller nothing.
- **Cause.** These commands bypass the command queue and are run by an
  engine; the C API reported `OSDP_COMPLETION_ACCEPTED` at hand-off.
- **Decision.** Fix upstream rather than model "accepted" in Rust.
- **Resolution.** Resolved upstream: 1994239 completes engine-owned
  commands when the engine finishes, eb02501 drops
  `OSDP_COMPLETION_ACCEPTED`. See the entry on tying an engine
  operation to its outcome.

## No handle tying an engine operation to its outcome

- **Symptom.** The real outcome of an engine-owned operation arrived as
  `OSDP_NOTIFICATION_MP_DONE`, with nothing linking it to the submission
  that started it. Two PIV submissions for the same element looked the
  same; correlating them relied on the one-op-per-PD rule, which the API
  did not promise.
- **Cause.** The lifecycle was split across two unrelated paths: an
  immediate completion and a later notification.
- **Decision.** Same fix as for the re-entrant ACCEPTED completions:
  complete the command when the operation ends, so the submission
  itself carries the outcome.
- **Resolution.** Resolved upstream by 1994239 and eb02501, with
  f76387e adding `osdp_cp_cancel()` for multi-part operations.
  libosdp-rs returns a `Submission` from `submit` whose `status()`
  reports that outcome.

## FLUSHED and ABORTED completions fire during teardown

- **Symptom.** `FLUSHED` and `ABORTED` completions run inside the flush
  and teardown drain loops. Application code reached from them could
  call back into the context while it was being torn down.
- **Cause.** Nothing stopped an `osdp_*` call made from inside teardown.
- **Decision.** Refuse such calls in C rather than document a
  restriction for every binding.
- **Resolution.** Resolved upstream: 618a555 refuses `osdp_*` calls made
  during teardown.

## One channel per context

- **Symptom.** The channel moved into the context, so a CP can no longer
  drive several channels from one context, as
  `ControlPanelBuilder::add_channel` used to allow.
- **Cause.** Intended upstream: a channel is the multi-drop bus, and a
  context drives one bus.
- **Decision.** Accepted. Rust follows the C model.
- **Resolution.** One `ControlPanel` per bus; an application with
  several buses builds several.

## Engine reply not ordered before its completion

- **Symptom.** For `PIVDATA`, `GENAUTH` and `CRAUTH`, the reply event and
  the command's completion arrived on separate paths, and the command
  could complete OK before the application had the reply it asked for.
  A submission that resolves into its reply had nothing to resolve with.
- **Cause.** The engine finished the operation before dispatching the
  reassembled reply, and `osdp.h` promised neither the order nor the
  one-op-per-PD rule.
- **Decision.** Fix upstream and promise both on
  `osdp_cp_submit_command()`.
- **Resolution.** Fixed upstream by 50c6824 for PIV and authenticate
  operations. Open: a multi-part `BIOREADR` still finishes before its
  reply is dispatched.

## No documented rules for calls made from callbacks

- **Symptom.** Context methods take `&self`, so application code in a
  callback (channel, file op, command handler) can reach its own context
  again. A nested refresh recurses through the channel without bound,
  and a nested `osdp_file_register_ops()` closes and replaces the file
  ops while one of them is running.
- **Cause.** `osdp.h` does not say which calls each callback may make,
  and C refuses nested calls only during teardown, not a nested
  `osdp_*_refresh()` or `osdp_file_register_ops()`.
- **Decision.** Guard in Rust now, since Rust also owns the boxes those
  calls would free, and propose the C fix.
- **Resolution.** Open. libosdp-rs marks a context busy around every
  call that can run application code: a nested `refresh` does nothing,
  and a nested `submit` or `register_file_ops` returns
  `Error::Rejected`. Proposed upstream: document, per callback, the
  calls allowed from it, and refuse a nested refresh and file-ops
  registration the way teardown is refused. The refresh half of the
  Rust guard becomes redundant once that lands.

## PD answers status queries by editing the command

- **Symptom.** For the status queries (`LSTAT`, `ISTAT`, `OSTAT`,
  `RSTAT`) the PD builds its reply from `cmd->status` after the command
  callback returns, so the application answers by editing the command
  in place. `pd_command_callback_t`'s doc says replies that carry data
  are submitted inline as events, and names only MFG and BIO.
- **Cause.** Two reply mechanisms, one of them undocumented.
- **Decision.** Hide the difference behind `Responder::reply` and
  propose the C fix.
- **Resolution.** Open. Proposed upstream: document the in-place
  contract on `pd_command_callback_t`, or accept an inline
  `OSDP_EVENT_STATUS` as the reply, as for MFGREP. Only the
  implementation of `Responder::reply` would change.

## osdp_close_fn_t documented as called once per PD

- **Symptom.** `osdp_close_fn_t` says the channel's close is called once
  per PD; in 4.0 `osdp_cp_teardown` calls it once per context.
- **Cause.** Comment left over from the per-PD channel model.
- **Decision.** Rust closes its channel once, when the context drops;
  propose the doc fix.
- **Resolution.** Open. Proposed upstream: fix the comment.
