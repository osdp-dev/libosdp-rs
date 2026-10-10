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

## Completions fire inside the flush and teardown loops

- **Symptom.** `FLUSHED` completions run inside
  `osdp_cp_flush_commands()` and `osdp_pd_flush_events()`, and
  `ABORTED` ones inside teardown. Application code reached from them
  could call back into the context while it was being drained or torn
  down.
- **Cause.** Nothing stopped an `osdp_*` call made from inside
  teardown. Re-entry from a flush was already safe: the queue is
  detached before it is drained (bb6c7d7), so a submission made from a
  `FLUSHED` completion joins a fresh queue.
- **Decision.** Refuse calls made during teardown in C rather than
  document a restriction for every binding; leave the flush loop as it
  is.
- **Resolution.** Resolved upstream: 618a555 refuses `osdp_*` calls
  made during teardown, and af6ae1b extends that to refresh. Later
  gaps closed the same way: dd9aad5 refuses `osdp_cp_add_pd()`,
  `osdp_file_register_ops()` and a nested refresh from a completion
  fired by a flush, which had been freeing the PD array the drain loop
  was walking, and 907443e refuses the two reshaping calls during
  teardown, documenting that only a completion teardown itself fired
  may call teardown again (and that call does nothing).
  libosdp-rs runs no application code from a completion either way; it
  only records the outcome for the `Submission`.

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
- **Resolution.** Resolved upstream: 50c6824 for PIV and authenticate
  operations, which also refuses chaining a new PIV or authenticate
  command from inside the reply event callback, and 02c088f for a
  multi-part `BIOREADR`, with the order documented on
  `struct osdp_cmd_bioread`. When another engine already runs an
  operation on the PD, the `BIOREAD` command completes at the first
  fragment as it does without the multi-part flag, so a `Submission`
  cannot count on the reassembled reply in that case; d3f743d corrects
  the `osdp.h` note, which had promised the order for that case too.

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
- **Resolution.** Resolved upstream: f1b0b43 marks the context running
  for the length of refresh and submit and refuses a nested refresh,
  `osdp_file_register_ops()` and `osdp_cp_add_pd()`, and documents on
  the refresh calls which calls a callback may make. 0efd64a then
  refuses `osdp_file_register_ops()` once the context has been
  refreshed, so file ops can no longer be replaced under a running
  transfer. libosdp-rs keeps its own guard, which also covers the
  boxes it owns: a nested `refresh` does nothing, and a nested
  `submit` returns `Error::Rejected`. File ops are given at setup,
  with `PdInfoBuilder::file_ops`, so a callback cannot register any.

## PD answers status queries by editing the command

- **Symptom.** For the status queries (`LSTAT`, `ISTAT`, `OSTAT`,
  `RSTAT`) the PD builds its reply from `cmd->status` after the command
  callback returns, so the application answers by editing the command
  in place. `pd_command_callback_t`'s doc says replies that carry data
  are submitted inline as events, and names only MFG and BIO.
- **Cause.** Two reply mechanisms, one of them undocumented.
- **Decision.** Hide the difference behind `Responder::reply` and
  propose the C fix.
- **Resolution.** Resolved upstream: 85af264 accepts an inline
  `OSDP_EVENT_STATUS` of the queried type, submitted from the command
  callback, as the reply, keeps the in-place report, and documents both
  on `pd_command_callback_t`. `Responder::reply` submits the event, so a
  report with the wrong entry count is refused by
  `osdp_pd_submit_event()` instead of turning into a NAK.

## osdp_close_fn_t documented as called once per PD

- **Symptom.** `osdp_close_fn_t` says the channel's close is called once
  per PD; in 4.0 `osdp_cp_teardown` calls it once per context.
- **Cause.** Comment left over from the per-PD channel model.
- **Decision.** Rust closes its channel once, when the context drops;
  propose the doc fix.
- **Resolution.** Resolved upstream by 9c57cf2, which says the channel
  is closed once per context.

## osdp_cp_add_pd() moves PDs under running operations

- **Symptom.** `osdp_cp_add_pd()` copies the PD array into a larger
  one and frees the old one. A file transfer, PIV, authenticate or
  multi-part biometric operation running on an existing PD keeps a
  pointer to that PD's old slot, and uses it when it next reports
  progress or completes. Called from inside a callback during refresh,
  it also frees the PD the refresh loop is working on.
- **Cause.** The feature engines bind their PD by address when an
  operation opens, and nothing re-binds them after the move; nested
  calls are not refused (see the entry on calls made from callbacks).
- **Decision.** Fix upstream before binding `osdp_cp_add_pd()`: re-bind
  the engines after the move, and refuse the call from inside a
  callback.
- **Resolution.** Resolved upstream: 740de1a re-binds each existing
  PD's engines to its new slot after the move, f1b0b43 refuses the
  call from inside a callback, dd9aad5 and 907443e from a flush
  completion and during teardown, and 35d519d stops it copying from a
  NULL array when the context has no PDs yet; dd33aff then refuses it
  once the CP has been refreshed (see the entry on adding a PD after
  setup). libosdp-rs does not bind it on a running CP: PDs are added
  only through `ControlPanelBuilder`, and a running CP takes PDs
  offline and back with `disable_pd` and `enable_pd`.

## PD ID and capabilities read the same before and after the PD reports them

- **Symptom.** `osdp_cp_get_pd_id()` and `osdp_cp_get_capability()`
  return 0 and a zeroed record for a PD that has never come online, the
  same as for a PD that reported zeros.
- **Cause.** The records are filled during the PD's INIT handshake, and
  nothing marks them as filled.
- **Decision.** Accepted. The handshake completes before a PD is
  reported online, so a record read while the PD is online is current.
- **Resolution.** libosdp-rs returns `None` from `pd_id` and
  `capability` while the PD is offline, and from `capability` for a
  function the PD did not advertise.

## CP leaves the application's file open when it refuses an empty file

- **Symptom.** A file transfer of a file that opens but reports size 0
  was refused, and the application's `close` never ran, leaving the
  file open.
- **Cause.** `osdp_file_tx_command()` opens the file through the
  application's ops before checking the size, and the refusal path
  returned before the transfer was marked open.
- **Decision.** Fix upstream; Rust's `FileOps` should see every `open`
  paired with a `close`.
- **Resolution.** Resolved upstream by bede75e, which closes the file
  on that refusal path.

## osdp_cp_is_pd_enabled() reports true for a bad PD index

- **Symptom.** An out-of-range PD index read back as enabled, against
  the documented `false` on error.
- **Cause.** The index check returns -1 on failure, which converts to
  `true` in a function returning `bool`.
- **Decision.** Fix upstream; the Rust wrapper should not second-guess
  the index.
- **Resolution.** Resolved upstream by 3a2afa6.

## Metrics header and implementation disagreed on what is counted

- **Symptom.** `struct osdp_metrics` says `sc_handshake_count` counts
  successful secure-channel activations and `sc_failure_count`
  tear-downs of an active session. The code counted a handshake when
  one started and a failure when a cryptogram did not verify, so a
  failed handshake bumped both and a session dropped by a bad MAC,
  timeout or re-key bumped neither.
- **Cause.** The counters were bumped at the handshake steps rather than
  where a side goes active or inactive.
- **Decision.** Make the code do what the header documents, so
  `Metrics` can carry the documented meaning.
- **Resolution.** Resolved upstream by 634b612, which counts in
  `sc_activate()` and `sc_deactivate()` and spells out on the fields
  what counts.

## PIN layout accepted at submit but refused later inside the session

- **Symptom.** A PIN-entry layout the wire cannot carry (unknown
  format, an offset not byte-aligned or beyond 15 bits, a length or
  block too large for its nibble) was accepted by submit and refused by
  the frame builder once the band was running, when the application
  could no longer act on it. `osdp.h` promises such a submission fails.
- **Cause.** Admission checked only the APDU length.
- **Decision.** Fix upstream so the error surfaces from `submit`.
- **Resolution.** Resolved upstream by 28cbc72, which checks the layout
  in `osdp_trs_pin_entry_check()` at admission and in the frame
  builder; c91b224 restores the frame builder's check of the encoder
  results, which optimized builds had flagged as possibly
  uninitialized.

## Presence-scan disable doc ahead of the code

- **Symptom.** The `osdp_cp_trs_scan_disable()` doc said the reader is
  restored to its default mode before the call returns.
- **Cause.** The code only clears the scan flags; the reader is
  restored on the next refresh.
- **Decision.** Fix the doc to match the code; Rust follows the code.
- **Resolution.** Resolved upstream by 84e49f2; Rust follows the
  code, so the reader is back in its default mode after the next
  refresh, not on return.

## Adding a PD is allowed after setup

- **Symptom.** `osdp_cp_add_pd()` is accepted at any time on a running
  CP, including from application code that runs inside
  `osdp_cp_add_pd()` itself (the logger, through the warnings and
  errors it logs after growing the PD array) and inside
  `osdp_file_register_ops()` (the file close of an aborted transfer).
  The outer call then goes on using a PD pointer into an array the
  nested call has reallocated.
- **Cause.** The call was added for building the PD list after
  `osdp_cp_setup()`, but nothing limits it to setup, so every path that
  runs application code while holding a PD pointer has to refuse it.
- **Decision.** Treat adding a PD as setup only. libosdp-rs adds PDs
  only through `ControlPanelBuilder`; upstream should refuse
  `osdp_cp_add_pd()` once the CP has been refreshed.
- **Resolution.** Resolved upstream by dd33aff, which records a
  context's first refresh and refuses `osdp_cp_add_pd()` after it, and
  marks the context running while it adds PDs, so a logger reached
  from there cannot add PDs underneath it. The path through
  `osdp_file_register_ops()` is gone too: 0efd64a refuses that call
  after the first refresh, before any transfer can start, and drops
  its abort of a live transfer.

## Setup calls accepted on a running context

- **Symptom.** `osdp_pd_set_capabilities()`, the four callback setters
  (`osdp_cp_set_event_callback()`,
  `osdp_cp_set_command_completion_callback()`,
  `osdp_pd_set_command_callback()`,
  `osdp_pd_set_event_completion_callback()`) and
  `osdp_file_register_ops()` were accepted at any time. A CP reads a
  PD's capabilities once, when the PD comes online, and plans what it
  sends from them; a completion callback swapped with commands in
  flight receives completions it never owned; file ops replaced during
  a transfer had to abort it, running the application's close from
  inside the call.
- **Cause.** Nothing marked the end of setup, and the capability and
  callback setters returned nothing, so they could not refuse.
- **Decision.** Treat them as setup only, as for adding a PD.
  libosdp-rs makes these calls only while it builds a context.
- **Resolution.** Resolved upstream: 3b7ea6e, b008a19 and 0efd64a
  refuse them with -1 once the context has been refreshed, the setters
  now returning `int`. libosdp-rs registers its callbacks, and the file
  ops given with `PdInfoBuilder::file_ops`, in
  `ControlPanelBuilder::build` and `PeripheralDevice::new`, which fail
  with `Error::Setup` when LibOSDP refuses one; `PdInfoBuilder` passes
  the capabilities to setup.
